use crate::config::AgentConfig;
use crate::integrations::Integration;
use crate::memory::Memory;
use crate::tool_call::executor::execute_tool;
use crate::tool_call::parser::extract_tool_call;
use eventsource_stream::Eventsource;
use futures_util::stream::StreamExt;
use reqwest::*;
use serde_json::{Value, json};
use std::io::{self, Write};
use tokio::sync::mpsc::UnboundedSender;

pub struct Agent {
    client: Client,
    pub config: AgentConfig,
    pub memory: Memory,
    pub active_memory: Option<(String, Memory)>,
    pub integrations: Vec<Integration>,
    pub history: Vec<Value>,
    use_openrouter: bool,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        system_prompt: String,
        root_memory: Memory,
        integrations: Vec<Integration>,
        use_openrouter: bool,
    ) -> Self {
        Self {
            client: Client::new(),
            history: vec![json!({
                "role": "system",
                "content": system_prompt
            })],
            config,
            memory: root_memory,
            active_memory: None,
            integrations,
            use_openrouter,
        }
    }

    pub async fn step(
        &mut self,
        user_input: &str,
        tx_token: UnboundedSender<String>,
    ) -> anyhow::Result<String> {
        self.history
            .push(json!({"role": "user", "content": user_input}));

        let mut consecutive_errors = 0;

        loop {
            // Context Window Management: keep system prompt and last max_history_messages.
            // FIX: always advance to the next "user" message after trimming so the
            // history never starts with an "assistant" turn (which llama.cpp rejects).
            if self.history.len() > self.config.max_history_messages + 1 {
                let candidate = self.history.len() - self.config.max_history_messages;
                let keep_from = (candidate..self.history.len())
                    .find(|&i| self.history[i]["role"].as_str() == Some("user"))
                    .unwrap_or(candidate);
                let mut new_history = vec![self.history[0].clone()];
                new_history.extend_from_slice(&self.history[keep_from..]);
                self.history = new_history;
            }

            let request = if self.use_openrouter {
                let api_key = self.config.openrouter_api_key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenRouter API key is missing from config [agent] openrouter_api_key"
                    )
                })?;
                self.client
                    .post("https://openrouter.ai/api/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", api_key))
            } else {
                self.client
                    .post(format!("{}/v1/chat/completions", self.config.base_url))
            };

            let mut stream = request
                .json(&json!({
                    "model": self.config.model,
                    "messages": self.history,
                    "temperature": self.config.temperature,
                    "max_tokens": self.config.max_tokens,
                    "stream": true
                }))
                .send()
                .await?
                .bytes_stream()
                .eventsource();

            let mut full_reply = String::new();
            let mut suppress_output = false;
            let mut printed_prefix = false;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(event) => {
                        if event.data == "[DONE]" {
                            break;
                        }
                        if let Ok(json) = serde_json::from_str::<Value>(&event.data) {
                            if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                                full_reply.push_str(content);

                                if full_reply.contains("<tool_call") {
                                    suppress_output = true;
                                }

                                if !suppress_output {
                                    if !printed_prefix {
                                        print!("aion: ");
                                        io::stdout().flush()?;
                                        printed_prefix = true;
                                    }
                                    print!("{}", content);
                                    io::stdout().flush()?;
                                    let _ = tx_token.send(content.to_string());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("\nStream error: {}", e);
                        break;
                    }
                }
            }

            if !suppress_output && printed_prefix {
                println!();
            }

            self.history
                .push(json!({"role": "assistant", "content": full_reply.clone()}));

            match extract_tool_call(&full_reply) {
                Ok(Some(call)) => {
                    println!("=> Executing tool: {}", call.name);
                    let result = execute_tool(
                        &call,
                        &self.memory,
                        &mut self.active_memory,
                        &self.integrations,
                    )
                    .await;
                    let response_xml = format!(
                        "<tool_response>\n{}\n</tool_response>",
                        serde_json::to_string_pretty(&result).unwrap_or_default()
                    );
                    // FIX: use "tool" role so the model's chat template doesn't count
                    // this as a user turn and reject back-to-back user messages.
                    self.history
                        .push(json!({"role": "tool", "content": response_xml}));
                    consecutive_errors = 0;
                }
                Err(e) => {
                    println!("=> Tool parse error: {}", e);
                    let response_xml = format!(
                        "<tool_response>\n{{\"error\": \"{}\"}}\n</tool_response>",
                        e.to_string().replace('"', "\\\"")
                    );
                    // FIX: use "tool" role for error responses too
                    self.history
                        .push(json!({"role": "tool", "content": response_xml}));

                    consecutive_errors += 1;
                    if consecutive_errors >= self.config.max_tool_errors {
                        println!("=> Max tool errors reached. Yielding to user.");
                        return Ok(format!("I encountered an error trying to use tools: {}", e));
                    }
                }
                Ok(None) => {
                    return Ok(full_reply);
                }
            }
        }
    }
}
