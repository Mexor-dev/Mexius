use crate::hermes::Message;
use tokio::process::Command;
use tokio::fs;
use serde_json::json;

/// Central tool runner for Mexius embedded tool functionality.
/// Accepts Hermes intents formatted as `run_tool:<name>`.
pub async fn run_tool(msg: &Message) -> Result<String, String> {
    if let Some(tool) = msg.intent.strip_prefix("run_tool:") {
        log::info!("Mexius tool dispatch: {}", tool);
        match tool {
            // ── shell ─────────────────────────────────────────────────────────
            "shell" | "terminal" => {
                let out = Command::new("sh")
                    .arg("-lc")
                    .arg(&msg.content)
                    .output()
                    .await
                    .map_err(|e| format!("Failed to spawn shell: {}", e))?;
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".into());
                Ok(format!("exit={} stdout='{}' stderr='{}'", code, stdout, stderr))
            }
            // ── read_file ─────────────────────────────────────────────────────
            "read_file" => {
                let path = msg.content.trim();
                let bytes = fs::read(path).await
                    .map_err(|e| format!("Failed to read {}: {}", path, e))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Ok(text)
            }
            // ── create_file / write_file ──────────────────────────────────────
            "create_file" | "write_file" => {
                if let Some((path, body)) = msg.content.split_once('\n') {
                    let path = path.trim();
                    // Ensure parent directory exists
                    if let Some(parent) = std::path::Path::new(path).parent() {
                        if !parent.as_os_str().is_empty() {
                            let _ = fs::create_dir_all(parent).await;
                        }
                    }
                    fs::write(path, body)
                        .await
                        .map_err(|e| format!("Failed to write {}: {}", path, e))?;
                    Ok(format!("Wrote {} bytes to: {}", body.len(), path))
                } else {
                    Err("create_file expects first line to be path, then a newline, then file contents".to_string())
                }
            }
            // ── append_file ───────────────────────────────────────────────────
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
                        Ok(format!("Appended {} bytes to: {}", data.len(), path))
                    })
                    .await
                    .map_err(|e| format!("Task join error: {}", e))?;
                    res
                } else {
                    Err("append_file expects first line to be path, then a newline, then file contents".to_string())
                }
            }
            // ── list_dir ──────────────────────────────────────────────────────
            "list_dir" => {
                let path = msg.content.trim();
                let path = if path.is_empty() { "." } else { path };
                let mut entries = Vec::new();
                let mut dir = fs::read_dir(path).await
                    .map_err(|e| format!("Failed to list {}: {}", path, e))?;
                while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    entries.push(if is_dir { format!("{}/", name) } else { name });
                }
                entries.sort();
                Ok(entries.join("\n"))
            }
            // ── glob_search / find_files ──────────────────────────────────────
            "glob_search" | "find_files" => {
                // content: "<pattern>" or "<dir>\n<pattern>"
                let (dir, pattern) = if let Some((d, p)) = msg.content.split_once('\n') {
                    (d.trim(), p.trim())
                } else {
                    (".", msg.content.trim())
                };
                if pattern.is_empty() {
                    return Err("glob_search: missing file pattern".to_string());
                }
                let out = Command::new("find")
                    .arg(if dir.is_empty() { "." } else { dir })
                    .arg("-name")
                    .arg(pattern)
                    .arg("-print")
                    .output()
                    .await
                    .map_err(|e| format!("find error: {}", e))?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    return Err(format!("glob_search failed: {}", if stderr.is_empty() { "unknown error" } else { &stderr }));
                }
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let lines = stdout
                    .lines()
                    .take(100)
                    .collect::<Vec<_>>();
                Ok(if lines.is_empty() { "(no matches)".to_string() } else { lines.join("\n") })
            }
            // ── git ───────────────────────────────────────────────────────────
            "git" => {
                // content: git subcommand + args e.g. "status" or "log --oneline -5"
                let args = msg.content.trim();
                let cmd = format!("git {}", args);
                let out = Command::new("sh").arg("-c").arg(&cmd).output().await
                    .map_err(|e| format!("git error: {}", e))?;
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let code = out.status.code().unwrap_or(-1);
                if code == 0 { Ok(stdout) } else { Err(format!("git exit={} stderr='{}'", code, stderr)) }
            }
            // ── http_get / web_fetch ──────────────────────────────────────────
            "http_get" | "web_fetch" => {
                let url = msg.content.trim().to_string();
                // Validate URL scheme for security
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err("http_get: only http:// and https:// URLs are allowed".to_string());
                }
                let body = reqwest::get(&url).await
                    .map_err(|e| format!("HTTP GET failed: {}", e))?
                    .text().await
                    .map_err(|e| format!("Failed to read response body: {}", e))?;
                // Truncate large responses
                let truncated = if body.len() > 8192 {
                    format!("{}\n...(truncated {} bytes total)", &body[..8192], body.len())
                } else {
                    body
                };
                Ok(truncated)
            }
            // ── web_search ─────────────────────────────────────────────────────
            "web_search" | "search_web" => {
                let query = msg.content.trim();
                if query.is_empty() {
                    return Err("web_search: missing query".to_string());
                }

                let provider = std::env::var("MEXIUS_WEB_SEARCH_PROVIDER")
                    .unwrap_or_else(|_| "duckduckgo".to_string())
                    .to_lowercase();
                let camofox_url = std::env::var("CAMOFOX_URL").ok();

                if provider == "camofox" {
                    if let Some(base) = camofox_url.as_deref() {
                        let endpoint = format!("{}/search", base.trim_end_matches('/'));
                        let url = reqwest::Url::parse_with_params(
                            &endpoint,
                            &[("q", query), ("limit", "8")],
                        )
                        .map_err(|e| format!("web_search(camofox): invalid URL: {}", e))?;

                        if let Ok(resp) = reqwest::get(url).await {
                            if resp.status().is_success() {
                                let body = resp.text().await.unwrap_or_default();
                                let truncated = if body.len() > 8192 {
                                    format!("{}\n...(truncated {} bytes total)", &body[..8192], body.len())
                                } else {
                                    body
                                };
                                return Ok(truncated);
                            }
                        }
                    }
                    return Err("web_search(camofox): provider selected but CAMOFOX_URL /search is unavailable".to_string());
                }

                let url = reqwest::Url::parse_with_params(
                    "https://api.duckduckgo.com/",
                    &[
                        ("q", query),
                        ("format", "json"),
                        ("no_redirect", "1"),
                        ("no_html", "1"),
                    ],
                )
                .map_err(|e| format!("web_search: invalid URL: {}", e))?;

                let body: serde_json::Value = reqwest::get(url)
                    .await
                    .map_err(|e| format!("web_search failed: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("web_search decode failed: {}", e))?;

                let mut out = Vec::new();
                out.push(format!("Search results for: {}", query));

                if let Some(abs) = body.get("AbstractText").and_then(|v| v.as_str()) {
                    if !abs.trim().is_empty() {
                        out.push(format!("Summary: {}", abs.trim()));
                    }
                }

                if let Some(topics) = body.get("RelatedTopics").and_then(|v| v.as_array()) {
                    let mut idx = 1usize;
                    for topic in topics {
                        if idx > 8 {
                            break;
                        }

                        if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                            let first_url = topic
                                .get("FirstURL")
                                .and_then(|v| v.as_str())
                                .unwrap_or("-");
                            out.push(format!("{}. {}", idx, text));
                            out.push(format!("   {}", first_url));
                            idx += 1;
                            continue;
                        }

                        if let Some(nested) = topic.get("Topics").and_then(|v| v.as_array()) {
                            for inner in nested {
                                if idx > 8 {
                                    break;
                                }
                                if let Some(text) = inner.get("Text").and_then(|v| v.as_str()) {
                                    let first_url = inner
                                        .get("FirstURL")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("-");
                                    out.push(format!("{}. {}", idx, text));
                                    out.push(format!("   {}", first_url));
                                    idx += 1;
                                }
                            }
                        }
                    }
                }

                if out.len() == 1 {
                    out.push("No results found.".to_string());
                }

                Ok(out.join("\n"))
            }

            // ── calendar ───────────────────────────────────────────────────────
            "calendar" => {
                let cmd = msg.content.trim();
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let cal_dir = format!("{}/.mexius", home);
                let cal_file = format!("{}/calendar_events.jsonl", cal_dir);

                if cmd.is_empty() || cmd.eq_ignore_ascii_case("help") {
                    return Ok("calendar usage:\n- calendar now\n- calendar list\n- calendar add <event text>".to_string());
                }

                if cmd.eq_ignore_ascii_case("now") || cmd.eq_ignore_ascii_case("today") {
                    let out = Command::new("sh")
                        .arg("-lc")
                        .arg("date '+%Y-%m-%d %H:%M:%S %Z'")
                        .output()
                        .await
                        .map_err(|e| format!("calendar now failed: {}", e))?;
                    return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
                }

                if cmd.eq_ignore_ascii_case("list") {
                    let content = tokio::fs::read_to_string(&cal_file)
                        .await
                        .unwrap_or_default();
                    let lines = content.lines().rev().take(50).collect::<Vec<_>>();
                    if lines.is_empty() {
                        return Ok("(no calendar events yet)".to_string());
                    }
                    let mut ordered = lines;
                    ordered.reverse();
                    return Ok(ordered.join("\n"));
                }

                if let Some(event_text) = cmd.strip_prefix("add ") {
                    tokio::fs::create_dir_all(&cal_dir)
                        .await
                        .map_err(|e| format!("calendar add failed to create dir: {}", e))?;
                    let ts = Command::new("sh")
                        .arg("-lc")
                        .arg("date -Is")
                        .output()
                        .await
                        .map_err(|e| format!("calendar add failed to timestamp: {}", e))?;
                    let ts = String::from_utf8_lossy(&ts.stdout).trim().to_string();
                    let line = json!({"timestamp": ts, "event": event_text.trim()}).to_string() + "\n";

                    tokio::task::spawn_blocking({
                        let cal_file = cal_file.clone();
                        let line = line.clone();
                        move || -> Result<(), String> {
                            use std::io::Write;
                            let mut f = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&cal_file)
                                .map_err(|e| format!("calendar add open error: {}", e))?;
                            f.write_all(line.as_bytes())
                                .map_err(|e| format!("calendar add write error: {}", e))?;
                            Ok(())
                        }
                    })
                    .await
                    .map_err(|e| format!("calendar add join error: {}", e))??;

                    return Ok(format!("calendar event added: {}", event_text.trim()));
                }

                Err("calendar: unsupported command. Use 'now', 'list', or 'add <event>'".to_string())
            }
            // ── status ────────────────────────────────────────────────────────
            "status" => Ok("mexius:ready".to_string()),
            // ── diagnostic ───────────────────────────────────────────────────
            "diagnostic" => {
                let mut report = serde_json::Map::new();

                match crate::compat::initialize().await {
                    Ok(msg) => { report.insert("initialize".to_string(), serde_json::Value::String(msg)); }
                    Err(e) => { report.insert("initialize".to_string(), serde_json::Value::String(format!("error: {}", e))); }
                }

                match crate::compat::init_tools().await {
                    Ok(tools_vec) => {
                        let tools_json = tools_vec.into_iter().map(|(n,ok)| json!({"name": n, "ok": ok})).collect::<Vec<_>>();
                        report.insert("tools".to_string(), serde_json::Value::Array(tools_json));
                    }
                    Err(e) => { report.insert("tools".to_string(), serde_json::Value::String(format!("error: {}", e))); }
                }

                let cargo_ok = Command::new("cargo").arg("--version").output().await.map(|o| o.status.success()).unwrap_or(false);
                report.insert("cargo".to_string(), serde_json::Value::Bool(cargo_ok));

                let port_ok = tokio::net::TcpListener::bind(("127.0.0.1", 42617)).await.is_ok();
                report.insert("port_42617_free".to_string(), serde_json::Value::Bool(port_ok));

                let home = std::env::var("HOME").unwrap_or_default();
                let pin_ok = tokio::fs::metadata(format!("{}/.mexius/pin.json", home)).await.is_ok();
                report.insert("pin_set".to_string(), serde_json::Value::Bool(pin_ok));

                let ollama_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
                report.insert("ollama_reachable".to_string(), serde_json::Value::Bool(ollama_ok));

                Ok(serde_json::Value::Object(report).to_string())
            }
            other => Err(format!("Unknown Mexius tool: '{}'. Available: shell, read_file, create_file, write_file, append_file, list_dir, glob_search, git, http_get, web_search, calendar, status, diagnostic", other)),
        }
    } else {
        Err("Unsupported intent format; expected 'run_tool:<name>'".to_string())
    }
}

