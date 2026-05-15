use crate::memory::Memory;
use crate::integrations::Integration;
use serde_json::{json, Value};
use crate::tool_call::parser::ToolCallRequest;
use std::process::{Command, Stdio};
use std::io::Write;

pub fn execute_tool(call: &ToolCallRequest, memory: &Memory, integrations: &[Integration]) -> Value {
    match call.name.as_str() {
        "memory_write" => {
            let key = call.arguments.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let value = call.arguments.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let scope = call.arguments.get("scope").and_then(|v| v.as_str()).unwrap_or("root");
            
            if key.is_empty() || value.is_empty() {
                return json!({"error": "key and value are required"});
            }
            
            match memory.write(key, value, scope) {
                Ok(_) => json!({"status": "success", "message": format!("Saved to scope: {}", scope)}),
                Err(e) => json!({"error": e.to_string()}),
            }
        },
        "memory_read" => {
            let key = call.arguments.get("key").and_then(|v| v.as_str()).unwrap_or("");
            match memory.read(key) {
                Ok(Some(val)) => json!({"value": val}),
                Ok(None) => json!({"error": "key not found"}),
                Err(e) => json!({"error": e.to_string()}),
            }
        },
        "memory_search" => {
            let query = call.arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            match memory.search(query) {
                Ok(results) => {
                    let results_json: Vec<_> = results.into_iter().map(|(k, v)| json!({"key": k, "value": v})).collect();
                    json!({"results": results_json})
                },
                Err(e) => json!({"error": e.to_string()}),
            }
        },
        "create_memory_scope" => {
            let name = call.arguments.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let reason = call.arguments.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            
            match memory.create_child(name, reason) {
                Ok(_) => json!({"status": "success", "message": format!("Created scope: {}", name)}),
                Err(e) => json!({"error": e.to_string()}),
            }
        },
        _ => {
            let mut matched_executor = None;
            for integration in integrations {
                if !integration.enabled { continue; }
                for tool in &integration.tools {
                    if tool.name == call.name {
                        matched_executor = Some(integration.executor.clone());
                        break;
                    }
                }
                if matched_executor.is_some() { break; }
            }

            match matched_executor {
                Some(executor_cmd) => {
                    let mut child = match Command::new("sh")
                        .arg("-c")
                        .arg(&executor_cmd)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn() {
                            Ok(c) => c,
                            Err(e) => return json!({"error": format!("Failed to spawn executor: {}", e)}),
                        };

                    if let Some(mut stdin) = child.stdin.take() {
                        let args_json = serde_json::to_string(&call.arguments).unwrap_or_default();
                        let _ = stdin.write_all(args_json.as_bytes());
                    }

                    match child.wait_with_output() {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            
                            if output.status.success() {
                                match serde_json::from_str::<Value>(&stdout) {
                                    Ok(json_val) => json_val,
                                    Err(_) => json!({
                                        "status": "success",
                                        "output": stdout.trim()
                                    })
                                }
                            } else {
                                json!({
                                    "error": "Tool execution failed",
                                    "exit_code": output.status.code(),
                                    "stdout": stdout.trim(),
                                    "stderr": stderr.trim()
                                })
                            }
                        },
                        Err(e) => json!({"error": format!("Failed to read command output: {}", e)}),
                    }
                },
                None => json!({"error": format!("Unknown tool: {}", call.name)})
            }
        }
    }
}
