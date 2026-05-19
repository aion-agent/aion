use serde_json::{json, Value};
use reqwest::*;
use eventsource_stream::Eventsource;
use futures_util::stream::StreamExt;
use std::io::{self, Write};
use crate::config::AgentConfig;
use crate::memory::Memory;
use crate::integrations::Integration;
use crate::tool_call::parser::extract_tool_call;
use crate::tool_call::executor::execute_tool;

pub struct Agent {
    client: Client,
    config: AgentConfig,
    pub memory: Memory,
    pub integrations: Vec<Integration>,
    history: Vec<Value>,
    use_openrouter: bool,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        system_prompt: String,
        memory: Memory,
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
            memory,
            integrations,
            use_openrouter,
        }
    }

    pub async fn step(&mut self, user_input: &str) -> anyhow::Result<String> {
        self.history.push(json!({"role": "user", "content": user_input}));

        let mut consecutive_errors = 0;

        loop {
            // Context Window Management: keep system prompt and last max_history_messages
            if self.history.len() > self.config.max_history_messages + 1 {
                let keep_from = self.history.len() - self.config.max_history_messages;
                let mut new_history = vec![self.history[0].clone()];
                new_history.extend_from_slice(&self.history[keep_from..]);
                self.history = new_history;
            }

            let request = if self.use_openrouter {
                let api_key = self.config.openrouter_api_key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("OpenRouter API key is missing from config [agent] openrouter_api_key")
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
                .send().await?
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

            self.history.push(json!({"role": "assistant", "content": full_reply.clone()}));

            match extract_tool_call(&full_reply) {
                Ok(Some(call)) => {
                    println!("=> Executing tool: {}", call.name);
                    let result = execute_tool(&call, &self.memory, &self.integrations);
                    let response_xml = format!("<tool_response>\n{}\n</tool_response>", serde_json::to_string_pretty(&result).unwrap_or_default());
                    
                    self.history.push(json!({"role": "user", "content": response_xml}));
                    consecutive_errors = 0;
                },
                Err(e) => {
                    println!("=> Tool parse error: {}", e);
                    let response_xml = format!("<tool_response>\n{{\"error\": \"{}\"}}\n</tool_response>", e.replace("\"", "\\\""));
                    self.history.push(json!({"role": "user", "content": response_xml}));
                    
                    consecutive_errors += 1;
                    if consecutive_errors >= self.config.max_tool_errors {
                        println!("=> Max tool errors reached. Yielding to user.");
                        return Ok(format!("I encountered an error trying to use tools: {}", e));
                    }
                },
                Ok(None) => {
                    return Ok(full_reply);
                }
            }
        }
    }
}
