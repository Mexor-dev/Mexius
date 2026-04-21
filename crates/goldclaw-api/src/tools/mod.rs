use crate::hermes::Message;
use tokio::process::Command;
use tokio::fs;

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
            other => Err(format!("Unknown embedded tool: {}", other)),
        }
    } else {
        Err("Unsupported intent format; expected 'run_tool:<name>'".to_string())
    }
}
