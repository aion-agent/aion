use crate::integrations::Integration;
use crate::memory::Memory;
use crate::tool_call::parser::ToolCallRequest;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

pub async fn execute_tool(
    call: &ToolCallRequest,
    root_memory: &Memory,
    active_memory: &mut Option<(String, Memory)>,
    integrations: &[Integration],
) -> Value {
    match call.name.as_str() {
        "memory_write" => {
            let key = call
                .arguments
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let value = call
                .arguments
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let scope = call
                .arguments
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("current");

            if key.is_empty() || value.is_empty() {
                return json!({"error": "key and value are required"});
            }

            let target_mem = if scope == "root" {
                root_memory
            } else {
                active_memory
                    .as_ref()
                    .map(|(_, conn)| conn)
                    .unwrap_or(root_memory)
            };

            match target_mem.write(key, value, scope) {
                Ok(_) => {
                    let active_name = active_memory
                        .as_ref()
                        .map(|(name, _)| name.as_str())
                        .unwrap_or("root");
                    let final_scope = if scope == "root" { "root" } else { active_name };
                    json!({"status": "success", "message": format!("Saved to scope: {}", final_scope)})
                }
                Err(e) => json!({"error": e.to_string()}),
            }
        }
        "memory_read" => {
            let key = call
                .arguments
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if key.is_empty() {
                return json!({"error": "key is required"});
            }

            let mut val_opt = None;
            let mut found_scope = "root";

            if let Some((name, conn)) = active_memory {
                match conn.read(key) {
                    Ok(Some(val)) => {
                        val_opt = Some(val);
                        found_scope = name;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        return json!({"error": format!("Error reading from active scope: {}", e)});
                    }
                }
            }

            if val_opt.is_none() {
                match root_memory.read(key) {
                    Ok(Some(val)) => {
                        val_opt = Some(val);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        return json!({"error": format!("Error reading from root scope: {}", e)});
                    }
                }
            }

            match val_opt {
                Some(val) => json!({"value": val, "scope": found_scope}),
                None => json!({"error": "key not found"}),
            }
        }
        "memory_search" => {
            let query = call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if query.is_empty() {
                return json!({"error": "query is required"});
            }

            let mut results_json = Vec::new();

            if let Some((name, conn)) = active_memory {
                match conn.search(query) {
                    Ok(results) => {
                        for (k, v) in results {
                            results_json.push(json!({
                                "key": k,
                                "value": v,
                                "scope": name.clone()
                            }));
                        }
                    }
                    Err(e) => {
                        return json!({"error": format!("Error searching active scope: {}", e)});
                    }
                }
            }

            match root_memory.search(query) {
                Ok(results) => {
                    for (k, v) in results {
                        if active_memory.is_none()
                            || results_json.iter().all(|r| r["key"].as_str() != Some(&k))
                        {
                            results_json.push(json!({
                                "key": k,
                                "value": v,
                                "scope": "root"
                            }));
                        }
                    }
                }
                Err(e) => return json!({"error": format!("Error searching root scope: {}", e)}),
            }

            json!({"results": results_json})
        }
        "create_memory_scope" => {
            let name = call
                .arguments
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = call
                .arguments
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if name.is_empty() {
                return json!({"error": "name is required"});
            }

            match root_memory.create_child(name, reason) {
                Ok(_) => {
                    json!({"status": "success", "message": format!("Created scope: {}", name)})
                }
                Err(e) => json!({"error": e.to_string()}),
            }
        }
        "switch_memory_scope" => {
            let name = call
                .arguments
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name.is_empty() {
                return json!({"error": "name is required"});
            }

            if name == "root" {
                *active_memory = None;
                json!({"status": "success", "message": "Switched back to root scope"})
            } else {
                match root_memory.list_children() {
                    Ok(children) => {
                        if let Some(child) = children.iter().find(|c| c.name == name) {
                            match Memory::open(Path::new(&child.path)) {
                                Ok(child_mem) => {
                                    *active_memory = Some((name.to_string(), child_mem));
                                    json!({"status": "success", "message": format!("Switched to scope: {}", name)})
                                }
                                Err(e) => {
                                    json!({"error": format!("Failed to open child database: {}", e)})
                                }
                            }
                        } else {
                            json!({"error": format!("Scope '{}' does not exist", name)})
                        }
                    }
                    Err(e) => json!({"error": format!("Failed to read scopes: {}", e)}),
                }
            }
        }
        "list_memory_scopes" => match root_memory.list_children() {
            Ok(children) => {
                let active_name = active_memory
                    .as_ref()
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("root");
                let children_json: Vec<_> = children
                    .into_iter()
                    .map(|c| {
                        let is_active = c.name == active_name;
                        json!({
                            "name": c.name,
                            "reason": c.reason,
                            "created_at": c.created_at,
                            "active": is_active
                        })
                    })
                    .collect();
                json!({
                    "active_scope": active_name,
                    "scopes": children_json
                })
            }
            Err(e) => json!({"error": e.to_string()}),
        },
        _ => {
            let mut matched_executor = None;
            for integration in integrations {
                if !integration.enabled {
                    continue;
                }
                for tool in &integration.tools {
                    if tool.name == call.name {
                        matched_executor = Some(integration.executor.clone());
                        break;
                    }
                }
                if matched_executor.is_some() {
                    break;
                }
            }

            match matched_executor {
                Some(executor_cmd) => {
                    let mut child = match tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(&executor_cmd)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                    {
                        Ok(c) => c,
                        Err(e) => {
                            return json!({"error": format!("Failed to spawn executor: {}", e)});
                        }
                    };

                    if let Some(mut stdin) = child.stdin.take() {
                        let args_json = serde_json::to_string(&call.arguments).unwrap_or_default();
                        let _ = stdin.write_all(args_json.as_bytes()).await;
                    }

                    match child.wait_with_output().await {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);

                            if output.status.success() {
                                match serde_json::from_str::<Value>(&stdout) {
                                    Ok(json_val) => json_val,
                                    Err(_) => json!({
                                        "status": "success",
                                        "output": stdout.trim()
                                    }),
                                }
                            } else {
                                json!({
                                    "error": "Tool execution failed",
                                    "exit_code": output.status.code(),
                                    "stdout": stdout.trim(),
                                    "stderr": stderr.trim()
                                })
                            }
                        }
                        Err(e) => json!({"error": format!("Failed to read command output: {}", e)}),
                    }
                }
                None => json!({"error": format!("Unknown tool: {}", call.name)}),
            }
        }
    }
}
