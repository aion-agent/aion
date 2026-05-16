use serde_json;
use serde::{Deserialize, Serialize};
use std::path::{PathBuf, Path};
use std::fs;

#[derive(Debug, Deserialize)]
pub struct IntegrationFile {
    integration: IntegrationMeta,
}

#[derive(Debug, Deserialize)]
pub struct IntegrationMeta {
    name: String,
    description: String,
    tools: String,      // path to tools json
    executor: String,
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Integration {
    pub name: String,
    pub description: String,
    pub tools: Vec<ToolSchema>,
    pub executor: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}



pub fn build_system_prompt(template_path: &Path, integrations: &[Integration]) -> String {
    let memory_tools = serde_json::json!([
        {
            "name": "memory_write",
            "description": "Store a key-value fact in memory",
            "parameters": {
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "value": { "type": "string" },
                    "scope": { "type": "string", "enum": ["root", "current"] }
                },
                "required": ["key", "value"]
            }
        },
        {
            "name": "memory_read",
            "description": "Read a value from memory by key",
            "parameters": {
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }
        },
        {
            "name": "memory_search",
            "description": "Full-text search across memory",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "create_memory_scope",
            "description": "Create a new isolated memory database for a project or task",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["name", "reason"]
            }
        }
    ]);

    let integration_tools: Vec<&ToolSchema> = integrations
        .iter()
        .filter(|i| i.enabled)
        .flat_map(|i| i.tools.iter())
        .collect();

    let all_tools = serde_json::json!([
        ..memory_tools.as_array().unwrap().clone(),
        ..integration_tools.iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect::<Vec<_>>()
    ]);

    let template = fs::read_to_string(template_path)
        .unwrap_or_else(|_| "Failed to load system prompt.".to_string());
        
    template.replace("{{TOOLS}}", &serde_json::to_string_pretty(&all_tools).unwrap())
}

pub fn load_integration(path: &Path) -> anyhow::Result<Integration> {
    let raw = fs::read_to_string(path)?;
    let file: IntegrationFile = toml::from_str(&raw)?;
    let meta = file.integration;

    if !meta.enabled {
        anyhow::bail!("integration disabled.");
    }

    let tools_raw = fs::read_to_string(&meta.tools)?;
    let tools: Vec<ToolSchema> = serde_json::from_str(&tools_raw)?;

    Ok(Integration {
        name: meta.name,
        description: meta.description,
        tools,
        executor: meta.executor,
        enabled: meta.enabled,
    })
}

pub fn load_all(dir: &Path) -> Vec<Integration> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "integration").unwrap_or(false))
        .filter_map(|e| load_integration(&e.path()).ok())
        .collect()
}
