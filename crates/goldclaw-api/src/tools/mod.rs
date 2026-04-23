use crate::hermes::Message;
use tokio::process::Command;
use tokio::fs;
use serde_json::json;

/// Central tool runner for embedded OpenClaw functionality.
/// Accepts Hermes intents formatted as `run_tool:<name>`.
pub async fn run_tool(msg: &Message) -> Result<String, String> {
    if let Some(tool) = msg.intent.strip_prefix("run_tool:") {
        log::info!("Embedded tool dispatch: {}", tool);
        match tool {
            "shell" => {
                let out = Command::new("sh")
                    .arg("-lc")
                    .arg(&msg.content)
                    .output()
                    .await
                    .map_err(|e| format!("Failed to spawn shell: {}", e))?;
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".into());
                Ok(format!("shell exit={} stdout='{}' stderr='{}'", code, stdout, stderr))
            }
            "create_file" | "write_file" => {
                if let Some((path, body)) = msg.content.split_once('\n') {
                    let path = path.trim();
                    fs::write(path, body)
                        .await
                        .map_err(|e| format!("Failed to write {}: {}", path, e))?;
                    Ok(format!("Wrote file: {}", path))
                } else {
                    Err("create_file expects first line to be path, then a newline, then file contents".to_string())
                }
            }
            "append_file" => {
                if let Some((path, body)) = msg.content.split_once('\n') {
                    let path = path.trim().to_string();
                    let data = body.as_bytes().to_vec();
                    let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                        use std::io::Write;
                        let mut opts = std::fs::OpenOptions::new();
                        opts.create(true).append(true);
                        let mut f = opts.open(&path).map_err(|e| format!("open error: {}", e))?;
                        f.write_all(&data).map_err(|e| format!("write error: {}", e))?;
                        Ok(format!("Appended to file: {}", path))
                    })
                    .await
                    .map_err(|e| format!("Task join error: {}", e))?;
                    res
                } else {
                    Err("append_file expects first line to be path, then a newline, then file contents".to_string())
                }
            }
            "status" => Ok("internal-openclaw:ready".to_string()),
            "diagnostic" => {
                // Run lightweight diagnostics reusing zeroclaw helpers where possible.
                let mut report = serde_json::Map::new();

                // Initialization probe
                match crate::zeroclaw::initialize().await {
                    Ok(msg) => { report.insert("initialize".to_string(), serde_json::Value::String(msg)); }
                    Err(e) => { report.insert("initialize".to_string(), serde_json::Value::String(format!("error: {}", e))); }
                }

                // Toolset probe
                match crate::zeroclaw::init_tools().await {
                    Ok(tools_vec) => {
                        let tools_json = tools_vec.into_iter().map(|(n,ok)| json!({"name": n, "ok": ok})).collect::<Vec<_>>();
                        report.insert("tools".to_string(), serde_json::Value::Array(tools_json));
                    }
                    Err(e) => { report.insert("tools".to_string(), serde_json::Value::String(format!("error: {}", e))); }
                }

                // Cargo availability
                let cargo_ok = Command::new("cargo").arg("--version").output().await.map(|o| o.status.success()).unwrap_or(false);
                report.insert("cargo".to_string(), serde_json::Value::Bool(cargo_ok));

                // Check gateway port availability (127.0.0.1:42617)
                let port_ok = tokio::net::TcpListener::bind(("127.0.0.1", 42617)).await.is_ok();
                report.insert("port_42617_free".to_string(), serde_json::Value::Bool(port_ok));

                // Config presence in current directory
                let cfg_exists = tokio::fs::metadata("config.toml").await.is_ok();
                report.insert("config_toml_exists".to_string(), serde_json::Value::Bool(cfg_exists));

                Ok(serde_json::Value::Object(report).to_string())
            }
            other => Err(format!("Unknown embedded tool: {}", other)),
        }
    } else {
        Err("Unsupported intent format; expected 'run_tool:<name>'".to_string())
    }
}
