use serde::Deserialize;
use std::path::Path;
use std::fs;

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
}

#[derive(Debug, Deserialize, Clone)]
pub struct IntegrationsConfig {
    pub discover: String,
}

fn default_temperature() -> f64 { 0.6 }
fn default_max_tokens() -> u32 { 1024 }
fn default_base_url() -> String { "http://localhost:8080".to_string() }

pub fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)?;
    let config: AppConfig = toml::from_str(&content)?;
    Ok(config)
}
