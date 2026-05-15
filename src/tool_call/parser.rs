use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

pub fn extract_tool_call(text: &str) -> Result<Option<ToolCallRequest>, String> {
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";
    
    if let Some(start_idx) = text.find(start_tag) {
        let content_after_start = &text[start_idx + start_tag.len()..];
        if let Some(end_idx) = content_after_start.find(end_tag) {
            let json_str = &content_after_start[..end_idx].trim();
            match serde_json::from_str::<ToolCallRequest>(json_str) {
                Ok(req) => return Ok(Some(req)),
                Err(e) => return Err(format!("Malformed JSON inside <tool_call>: {}", e)),
            }
        } else {
            return Err("Missing </tool_call> closing tag.".to_string());
        }
    }
    Ok(None)
}
