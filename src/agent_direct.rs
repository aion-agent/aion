use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use reqwest::*;
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
}

impl Agent {
    pub fn new(config: AgentConfig, system_prompt: String, memory: Memory, integrations: Vec<Integration>) -> Self {
        Self {
            client: Client::new(),
            history: vec![json!({
                "role": "system",
                "content": system_prompt
            })],
            config,
            memory,
            integrations,
        }
    }

    pub async fn step(&mut self, user_input: &str) -> anyhow::Result<String> {
        self.history.push(json!({"role": "user", "content": user_input}));

        loop {
            let res = self.client
                .post(format!("{}/v1/chat/completions", self.config.base_url))
                .json(&json!({
                    "model": self.config.model,
                    "messages": self.history,
                    "temperature": self.config.temperature,
                    "max_tokens": self.config.max_tokens
                }))
                .send().await?
                .json::<Value>().await?;
            
            let reply = res["choices"][0]["message"]["content"]
                .as_str().unwrap_or("").to_string();

            self.history.push(json!({"role": "assistant", "content": reply.clone()}));

            match extract_tool_call(&reply) {
                Ok(Some(call)) => {
                    println!("=> Executing tool: {}", call.name);
                    let result = execute_tool(&call, &self.memory, &self.integrations);
                    let response_xml = format!("<tool_response>\n{}\n</tool_response>", serde_json::to_string_pretty(&result).unwrap_or_default());
                    
                    self.history.push(json!({"role": "tool", "content": response_xml}));
                },
                Err(e) => {
                    println!("=> Tool parse error: {}", e);
                    let response_xml = format!("<tool_response>\n{{\"error\": \"{}\"}}\n</tool_response>", e.replace("\"", "\\\""));
                    self.history.push(json!({"role": "tool", "content": response_xml}));
                },
                Ok(None) => {
                    return Ok(reply);
                }
            }
        }
    }
}
