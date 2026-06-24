use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub integrations: IntegrationsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub model: String,
    pub memory: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_max_history_messages")]
    pub max_history_messages: usize,
    #[serde(default = "default_max_tool_errors")]
    pub max_tool_errors: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    pub openrouter_model: Option<String>,
    pub openrouter_api_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IntegrationsConfig {
    pub discover: String,
}

fn default_temperature() -> f64 {
    0.6
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_max_history_messages() -> usize {
    20
}
fn default_max_tool_errors() -> usize {
    3
}
fn default_system_prompt() -> String {
    "system_prompt.txt".to_string()
}

pub fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)?;
    let config: AppConfig = toml::from_str(&content)?;
    Ok(config)
}
