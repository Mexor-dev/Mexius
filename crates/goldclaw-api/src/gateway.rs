use hyper::{service::service_fn, Body, Request, Response, Method, StatusCode, server::conn::Http};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, broadcast};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::{SystemTime, UNIX_EPOCH};
use hyper::body::Bytes;
use serde::Deserialize;
use goldclaw_memory::MemoryCategory as GMemoryCategory;

const INDEX_HTML: &str = r#"<!doctype html>
<html lang=\"en\">
<head>
    <meta charset=\"utf-8\">
    <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">
    <title>Herma Dashboard</title>
    <link rel=\"stylesheet\" href=\"/styles.css\">
    <style>body{margin:0;padding:0}</style>
</head>
<body>
    <div id=\"app\" class=\"app\">
        <header class=\"hdr\"><h1>Herma</h1><div class=\"status\" id=\"status\">disconnected</div></header>
        <main>
            <section style=\"display:flex;gap:12px;padding:12px\">
                <div style=\"flex:1\"> 
                    <h3>System Health</h3>
                    <div id=\"health\">Loading...</div>
                    <div style=\"margin:8px 0;font-size:13px;">External IP: <span id=\"external_ip\">...</span></div>
                    <button id=\"reboot\">Reboot Entity</button>
                </div>
                <div style=\"flex:2\"> 
                    <h3>Logs</h3>
                    <div id=\"log\" class=\"log\"></div>
                    <div class=\"controls\">
                        <input id=\"cmd\" placeholder=\"Enter command (shell)\" />
                        <button id=\"run\">Run</button>
                    </div>
                </div>
            </section>
        </main>
    </div>
    <script src=\"/app.js\"></script>
</body>
</html>"#;

const STYLES_CSS: &str = r#"body{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Roboto Mono,monospace;background:#071017;color:#cfe8ff;height:100vh;display:flex;flex-direction:column}
.hdr{display:flex;align-items:center;justify-content:space-between;padding:12px 16px;background:#061018;border-bottom:1px solid #0f2a3a}
.log{flex:1;padding:12px 16px;overflow:auto;background:#071017}
.log div{padding:2px 0}
.controls{display:flex;padding:8px;border-top:1px solid #0f2a3a}
.controls input{flex:1;padding:8px;border-radius:4px;border:1px solid #123}
.controls button{margin-left:8px;padding:8px 12px;border-radius:4px}
#status{font-size:12px;color:#7ff}
"#;

const APP_JS: &str = r#"document.addEventListener('DOMContentLoaded',()=>{const log=document.getElementById('log');const status=document.getElementById('status');const health=document.getElementById('health');const es=new EventSource('/logs');es.onopen=()=>{status.textContent='connected'};es.onerror=()=>{status.textContent='error'};es.onmessage=(e)=>{const d=document.createElement('div');d.textContent=e.data;log.appendChild(d);log.scrollTop=log.scrollHeight};async function refreshHealth(){try{const r=await fetch('/api/system/health');const j=await r.json();health.innerHTML=`<pre>${JSON.stringify(j,null,2)}</pre>`;if(j && j.external_ip){const ip=document.getElementById('external_ip');if(ip) ip.textContent=j.external_ip;}}catch(err){health.textContent='Health check failed: '+err}};refreshHealth();setInterval(refreshHealth,5000);document.getElementById('run').addEventListener('click',async ()=>{const v=document.getElementById('cmd').value;try{const res=await fetch('/api/command',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({intent:'run_tool:shell',content:v})});const txt=await res.text();const d=document.createElement('div');d.textContent='> '+v;log.appendChild(d);const o=document.createElement('div');o.textContent='<= '+txt;log.appendChild(o);log.scrollTop=log.scrollHeight}catch(err){const e=document.createElement('div');e.textContent='ERROR: '+err;log.appendChild(e)}});document.getElementById('cmd').addEventListener('keydown',e=>{if(e.key==='Enter'){document.getElementById('run').click();e.preventDefault()}});document.getElementById('reboot').addEventListener('click',async ()=>{if(!confirm('Restart Herma service?')) return;try{const r=await fetch('/api/system/reboot',{method:'POST'});const t=await r.text();const e=document.createElement('div');e.textContent='Reboot: '+t;log.appendChild(e)}catch(err){const e=document.createElement('div');e.textContent='Reboot failed: '+err;log.appendChild(e)}});});"#;

#[derive(Deserialize)]
struct UiCommand {
    intent: Option<String>,
    content: Option<String>,
    id: Option<String>,
}

/// Run the Hyper gateway and serve embedded UI assets. The gateway receives
/// `tx` so it can forward UI commands into the Hermes channel, and `log_tx`
/// is used to stream server logs/events to connected UI clients over SSE.
pub async fn run(
    addr: SocketAddr,
    tx: mpsc::Sender<crate::hermes::Message>,
    log_tx: broadcast::Sender<String>,
    thought_tx: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Ensure Herma data directory exists (for LanceDB storage)
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let herma_path = std::path::Path::new(&home).join(".herma");
    let lancedb_path = herma_path.join("lancedb_store");
    match tokio::fs::create_dir_all(&lancedb_path).await {
        Ok(()) => log::info!("Ensured Herma data dir: {:?}", lancedb_path),
        Err(e) => log::warn!("Failed to create Herma data dir {:?}: {}", lancedb_path, e),
    }

    // Repo root; allow override with HERMA_ROOT env var (useful after renaming repo)
    let repo_root = Arc::new(std::env::var("HERMA_ROOT").unwrap_or_else(|_| format!("{}/herma", home)));
    // Make a clonable, thread-safe function object for resolving paths so it
    // can be moved into service closures without ownership issues.
    let resolve_repo_path: Arc<dyn Fn(&str) -> String + Send + Sync> = {
        let repo_root = repo_root.clone();
        Arc::new(move |p: &str| {
            if p.starts_with("/home/user/goldclaw") {
                p.replacen("/home/user/goldclaw", repo_root.as_str(), 1)
            } else {
                p.to_string()
            }
        })
    };

    let listener = TcpListener::bind(addr).await?;
    log::info!("Gateway listening on http://{}", addr);

    loop {
        let (stream, _peer) = listener.accept().await?;

        // Per-connection clones
        let tx_conn = tx.clone();
        let log_tx_conn = log_tx.clone();
        let thought_tx_conn = thought_tx.clone();
        let resolve_repo_path_conn = resolve_repo_path.clone();

        let svc = service_fn(move |req: Request<Body>| {
            let tx_req = tx_conn.clone();
            let log_tx_req = log_tx_conn.clone();
            let thought_tx_req = thought_tx_conn.clone();
            let resolve_repo_path = resolve_repo_path_conn.clone();
            async move {
                match (req.method(), req.uri().path()) {
                    (&Method::GET, "/") => {
                        // Prefer serving the built web UI if present on disk (web/dist/index.html)
                        let index_pstr = (resolve_repo_path)("/home/user/goldclaw/web/dist/index.html");
                        let index_path = std::path::Path::new(&index_pstr);
                        if let Ok(meta) = tokio::fs::metadata(index_path).await {
                            if meta.is_file() {
                                if let Ok(bytes) = tokio::fs::read(&index_path).await {
                                    let mut res = Response::new(Body::from(bytes));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/html; charset=utf-8"));
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }

                        // Fallback to embedded index
                        Ok::<_, hyper::Error>(Response::new(Body::from(INDEX_HTML)))
                    }
                    (&Method::GET, "/styles.css") => {
                        let mut res = Response::new(Body::from(STYLES_CSS));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/css; charset=utf-8"));
                        Ok(res)
                    }
                    (&Method::GET, "/app.js") => {
                        let mut res = Response::new(Body::from(APP_JS));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/javascript; charset=utf-8"));
                        Ok(res)
                    }
                    (&Method::GET, "/logs") => {
                        // SSE via Body::channel()
                        let (mut sender, body) = Body::channel();

                        // Spawn a task to forward broadcast messages into the SSE channel
                        tokio::spawn(async move {
                            let mut rx = log_tx_req.subscribe();
                            // initial welcome
                            let _ = sender.send_data(Bytes::from("event: log\ndata: connected\n\n")).await;
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        let line = format!("event: log\ndata: {}\n\n", msg.replace('\n', "\\n"));
                                        if let Err(err) = sender.send_data(Bytes::from(line)).await {
                                            log::debug!("SSE send error: {:?}", err);
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        let line = format!("event: system\ndata: [lagged {} messages]\n\n", n);
                                        let _ = sender.send_data(Bytes::from(line)).await;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });

                        let mut res = Response::new(body);
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/event-stream"));
                        res.headers_mut().insert(hyper::header::CACHE_CONTROL, hyper::header::HeaderValue::from_static("no-cache"));
                        Ok(res)
                    }
                    // SSE audit stream: real-time Hermes thought/action events
                    (&Method::GET, "/api/audit/stream") | (&Method::GET, "/api/audit.json") => {
                        // SSE via Body::channel(), subscribe to thought broadcaster
                        let (mut sender, body) = Body::channel();

                        tokio::spawn(async move {
                            let mut rx = thought_tx_req.subscribe();
                            // initial welcome
                            let _ = sender.send_data(Bytes::from("event: system\ndata: connected\n\n")).await;
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        // Attempt to parse JSON and extract `type` and `id`
                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg) {
                                            let ev_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("thought");
                                            let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                            let data = v.to_string().replace('\n', "\\n");
                                            let mut line = String::new();
                                            line.push_str(&format!("event: {}\n", ev_type));
                                            if !id.is_empty() {
                                                line.push_str(&format!("id: {}\n", id));
                                            }
                                            line.push_str(&format!("data: {}\n\n", data));
                                            if let Err(err) = sender.send_data(Bytes::from(line)).await {
                                                log::debug!("SSE send error: {:?}", err);
                                                break;
                                            }
                                        } else {
                                            // Fallback: send raw data as a thought event
                                            let line = format!("event: thought\ndata: {}\n\n", msg.replace('\n', "\\n"));
                                            if let Err(err) = sender.send_data(Bytes::from(line)).await {
                                                log::debug!("SSE send error: {:?}", err);
                                                break;
                                            }
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        let line = format!("event: system\ndata: [lagged {} messages]\n\n", n);
                                        let _ = sender.send_data(Bytes::from(line)).await;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });

                        let mut res = Response::new(body);
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/event-stream"));
                        res.headers_mut().insert(hyper::header::CACHE_CONTROL, hyper::header::HeaderValue::from_static("no-cache"));
                        Ok(res)
                    }
                    (&Method::GET, "/api/system/health") => {
                        // Health check: ollama socket, hermes (local), memory, external IP
                        let mut health = serde_json::Map::new();

                        // Check Ollama and probe model VRAM state via HTTP /api/show
                        let client = reqwest::Client::new();
                        let mut vram_loaded = false;
                        let mut model_name = serde_json::Value::String("".to_string());
                        match client.post("http://127.0.0.1:11434/api/show").json(&serde_json::json!({"model":"gemma-4-uncensored"})).send().await {
                            Ok(resp) => {
                                if let Ok(j) = resp.json::<serde_json::Value>().await {
                                    // Try common shapes
                                    if let Some(loaded) = j.get("vram").and_then(|v| v.get("loaded")).and_then(|b| b.as_bool()) {
                                        vram_loaded = loaded;
                                    }
                                    if vram_loaded == false {
                                        // try top-level `loaded` or models list
                                        if let Some(b) = j.get("loaded").and_then(|b| b.as_bool()) { vram_loaded = b }
                                        if let Some(models) = j.get("models").and_then(|m| m.as_array()) {
                                            for entry in models {
                                                if entry.get("name").and_then(|n| n.as_str()) == Some("gemma-4-uncensored") {
                                                    if let Some(ld) = entry.get("loaded").and_then(|b| b.as_bool()) {
                                                        vram_loaded = ld;
                                                    }
                                                    if let Some(nm) = entry.get("name").and_then(|n| n.as_str()) {
                                                        model_name = serde_json::Value::String(nm.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        if let Some(nm) = j.get("model").and_then(|m| m.as_str()) { model_name = serde_json::Value::String(nm.to_string()); }
                                    }
                                }
                                health.insert("ollama".to_string(), serde_json::Value::Bool(true));
                            }
                            Err(_) => {
                                // Fallback to TCP connect indicator
                                let ollama_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
                                health.insert("ollama".to_string(), serde_json::Value::Bool(ollama_ok));
                            }
                        }

                        health.insert("model_name".to_string(), model_name);
                        health.insert("vram_loaded".to_string(), serde_json::Value::Bool(vram_loaded));

                        // Hermes loop - local indicator (we assume gateway is running alongside Hermes)
                        health.insert("hermes_loop".to_string(), serde_json::Value::Bool(true));

                        // Memory: read /proc/meminfo if available
                        let mem = match tokio::fs::read_to_string("/proc/meminfo").await {
                            Ok(s) => {
                                let mut total = 0u64;
                                let mut avail = 0u64;
                                for line in s.lines() {
                                    if line.starts_with("MemTotal:") { let parts: Vec<&str> = line.split_whitespace().collect(); if parts.len()>1 { total = parts[1].parse::<u64>().unwrap_or(0); } }
                                    if line.starts_with("MemAvailable:") { let parts: Vec<&str> = line.split_whitespace().collect(); if parts.len()>1 { avail = parts[1].parse::<u64>().unwrap_or(0); } }
                                }
                                serde_json::json!({"total_kb": total, "available_kb": avail})
                            }
                            Err(_) => serde_json::json!(null),
                        };
                        health.insert("memory".to_string(), mem);

                        // External IP (detect without external crate)
                        let ip = {
                            match std::net::UdpSocket::bind("0.0.0.0:0") {
                                Ok(sock) => {
                                    if sock.connect("8.8.8.8:80").is_ok() {
                                        match sock.local_addr() {
                                            Ok(local) => local.ip().to_string(),
                                            Err(_) => "127.0.0.1".to_string(),
                                        }
                                    } else {
                                        "127.0.0.1".to_string()
                                    }
                                }
                                Err(_) => "127.0.0.1".to_string(),
                            }
                        };
                        health.insert("external_ip".to_string(), serde_json::Value::String(ip));

                        let body = serde_json::Value::Object(health);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        Ok(res)
                    }
                    (&Method::POST, "/api/system/reboot") => {
                        // Restart the herma service. Prefer systemctl in GUI/session
                        // environments; fall back to a direct kill+spawn in non-interactive
                        // WSL sessions where DBUS is not available.
                        let out = if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
                            match tokio::process::Command::new("systemctl")
                                .arg("--user")
                                .arg("restart")
                                .arg("herma-gateway")
                                .output()
                                .await
                            {
                                Ok(o) => format!("status: {}", o.status),
                                Err(e) => format!("error: {}", e),
                            }
                        } else {
                            // Non-systemd fallback: try a best-effort pkill + nohup spawn
                            let bin = if std::path::Path::new("/usr/local/bin/herma-gateway").exists() {
                                "/usr/local/bin/herma-gateway".to_string()
                            } else {
                                // repo-local candidate
                                "/home/user/herma/target/release/herma-gateway".to_string()
                            };
                            let cmd = format!(
                                "pkill -f herma-gateway || true; nohup {} gateway > /home/user/herma/gateway.log 2>&1 &",
                                bin
                            );
                            // Use non-blocking std::process spawn so reboot does not hang
                            match std::process::Command::new("sh").arg("-c").arg(cmd).spawn() {
                                Ok(_) => format!("fallback: spawned"),
                                Err(e) => format!("fallback error: {}", e),
                            }
                        };
                        let res = Response::new(Body::from(out));
                        Ok(res)
                    }
                    (&Method::POST, "/api/command") => {
                        // Read full body
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let cmd: Result<UiCommand, _> = serde_json::from_slice(&whole);
                        match cmd {
                            Ok(c) => {
                                let id = c.id.unwrap_or_else(|| {
                                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                                    format!("ui-{}", now.as_millis())
                                });
                                let intent = c.intent.unwrap_or_else(|| "run_tool:shell".to_string());
                                let content = c.content.unwrap_or_default();

                                let message = crate::hermes::Message { id: id.clone(), content, intent };

                                // Forward to Hermes asynchronously
                                let tx_send = tx_req.clone();
                                let log_send = log_tx_req.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = tx_send.send(message).await {
                                        let warn = format!("Failed to forward UI command to Hermes: {}", e);
                                        let _ = log_send.send(warn);
                                    }
                                });

                                let body = serde_json::json!({"status":"ok","id": id});
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                Ok(res)
                            }
                            Err(_) => {
                                let mut bad = Response::new(Body::from("Invalid JSON"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                Ok(bad)
                            }
                        }
                    }

                    // Serve disk-backed API JSON if present (dist/api or dev public/api),
                    // otherwise return dynamic, runtime-backed responses.
                    (&Method::GET, "/api/doctor.json") => {
                        // Try disk locations first
                        let candidates = [
                            "/home/user/goldclaw/web/dist/api/doctor.json",
                            "/mnt/c/Users/User/zero-ui/public/api/doctor.json",
                        ];
                        for p in &candidates {
                            let pstr = resolve_repo_path(p);
                            let path = std::path::Path::new(&pstr);
                            if let Ok(m) = tokio::fs::metadata(path).await {
                                if m.is_file() {
                                    if let Ok(bytes) = tokio::fs::read(path).await {
                                        let mut res = Response::new(Body::from(bytes));
                                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                        return Ok::<_, hyper::Error>(res);
                                    }
                                }
                            }
                        }

                        // Dynamic doctor info: reuse health checks and add metadata
                        let mut doc = serde_json::Map::new();

                        // Ollama reachable?
                        let ollama_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
                        doc.insert("ollama".to_string(), serde_json::Value::Bool(ollama_ok));

                        // Hermes loop indicator
                        doc.insert("hermes_loop".to_string(), serde_json::Value::Bool(true));

                        // Memory info
                        let mem = match tokio::fs::read_to_string("/proc/meminfo").await {
                            Ok(s) => {
                                let mut total = 0u64;
                                let mut avail = 0u64;
                                for line in s.lines() {
                                    if line.starts_with("MemTotal:") { let parts: Vec<&str> = line.split_whitespace().collect(); if parts.len()>1 { total = parts[1].parse::<u64>().unwrap_or(0); } }
                                    if line.starts_with("MemAvailable:") { let parts: Vec<&str> = line.split_whitespace().collect(); if parts.len()>1 { avail = parts[1].parse::<u64>().unwrap_or(0); } }
                                }
                                serde_json::json!({"total_kb": total, "available_kb": avail})
                            }
                            Err(_) => serde_json::json!(null),
                        };
                        doc.insert("memory".to_string(), mem);

                        // External IP
                        let ip = match std::net::UdpSocket::bind("0.0.0.0:0") {
                            Ok(sock) => {
                                if sock.connect("8.8.8.8:80").is_ok() {
                                    match sock.local_addr() { Ok(local) => local.ip().to_string(), Err(_) => "127.0.0.1".to_string() }
                                } else { "127.0.0.1".to_string() }
                            }
                            Err(_) => "127.0.0.1".to_string(),
                        };
                        doc.insert("external_ip".to_string(), serde_json::Value::String(ip));

                        // Uptime (from /proc/uptime)
                        if let Ok(s) = tokio::fs::read_to_string("/proc/uptime").await {
                            if let Some(sec) = s.split_whitespace().next() {
                                if let Ok(f) = sec.parse::<f64>() { doc.insert("uptime_seconds".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)))); }
                            }
                        }

                        let body = serde_json::Value::Object(doc);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        return Ok::<_, hyper::Error>(res);
                    }

                    // removed: /api/audit.json handled above as SSE stream

                    (&Method::GET, "/api/memory") => {
                        // Backward-compatible endpoint used by the SPA: /api/memory
                        // Disk-first
                        let candidates = [
                            "/home/user/goldclaw/web/dist/api/memory.json",
                            "/mnt/c/Users/User/zero-ui/public/api/memory.json",
                        ];
                        for p in &candidates {
                            let pstr = resolve_repo_path(p);
                            let path = std::path::Path::new(&pstr);
                            if let Ok(m) = tokio::fs::metadata(path).await {
                                if m.is_file() {
                                    if let Ok(bytes) = tokio::fs::read(path).await {
                                        let mut res = Response::new(Body::from(bytes));
                                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                        return Ok::<_, hyper::Error>(res);
                                    }
                                }
                            }
                        }

                        // Parse simple query params (best-effort, no percent-decoding)
                        let mut q_param: Option<String> = None;
                        let mut category_param: Option<String> = None;
                        if let Some(qs) = req.uri().query() {
                            for pair in qs.split('&') {
                                if pair.is_empty() { continue; }
                                let mut it = pair.splitn(2, '=');
                                let k = it.next().unwrap_or("");
                                let v = it.next().unwrap_or("");
                                match k {
                                    "query" => q_param = Some(v.to_string()),
                                    "category" => category_param = Some(v.to_string()),
                                    _ => {}
                                }
                            }
                        }

                        // Try live memory backend (Sqlite) across candidate workspaces
                        let ws_candidates = [
                            "/home/user/.openclaw",
                            "/home/user",
                            "/home/user/Project/zeroclaw",
                            "/home/user/goldclaw",
                        ];

                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = goldclaw_memory::SqliteMemory::new(p) {
                                // If query provided, run recall, otherwise list
                                let entries_res = if let Some(ref q) = q_param {
                                    mem.recall(q.as_str(), 50, None, None, None).await
                                } else {
                                    mem.list(None, None).await
                                };

                                if let Ok(mut entries) = entries_res {
                                    // Apply category filter client-side if requested
                                    if let Some(ref cat) = category_param {
                                        entries.retain(|e| e.category.to_string() == *cat);
                                    }

                                    let body = match serde_json::to_string(&entries) {
                                        Ok(s) => s,
                                        Err(_) => "[]".to_string(),
                                    };
                                    let mut res = Response::new(Body::from(body));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }

                        // Final fallback: in-memory pinned fragments
                        match crate::memory_store::top_pinned(10).await {
                            items => {
                                let body = match serde_json::to_string(&items) {
                                    Ok(s) => s,
                                    Err(_) => "[]".to_string(),
                                };
                                let mut res = Response::new(Body::from(body));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                return Ok::<_, hyper::Error>(res);
                            }
                        }
                    }

                    (&Method::POST, "/api/memory") => {
                        // Store memory (expects JSON { key, content, category?, session_id? })
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&whole) {
                            let key = v.get("key").and_then(|s| s.as_str()).unwrap_or("");
                            let content = v.get("content").and_then(|s| s.as_str()).unwrap_or("");
                            let category = v.get("category").and_then(|s| s.as_str()).unwrap_or("core");
                            let session = v.get("session_id").and_then(|s| s.as_str());

                            if key.is_empty() || content.is_empty() {
                                let mut bad = Response::new(Body::from("Missing key or content"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                return Ok::<_, hyper::Error>(bad);
                            }

                            // Map category string -> MemoryCategory
                            let cat_enum = match category {
                                "core" => GMemoryCategory::Core,
                                "daily" => GMemoryCategory::Daily,
                                "conversation" => GMemoryCategory::Conversation,
                                other => GMemoryCategory::Custom(other.to_string()),
                            };

                            let ws_candidates = [
                                "/home/user/.openclaw",
                                "/home/user",
                                "/home/user/Project/zeroclaw",
                                "/home/user/goldclaw",
                            ];

                            let mut stored = false;
                            for ws in &ws_candidates {
                                let pstr = resolve_repo_path(ws);
                                let p = std::path::Path::new(&pstr);
                                if let Ok(mem) = goldclaw_memory::SqliteMemory::new(p) {
                                    match mem.store(key, content, cat_enum.clone(), session).await {
                                        Ok(()) => { stored = true; break; }
                                        Err(_) => continue,
                                    }
                                }
                            }

                            let body = serde_json::json!({"ok": stored});
                            let mut res = Response::new(Body::from(body.to_string()));
                            res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                            return Ok::<_, hyper::Error>(res);
                        }

                        let mut bad = Response::new(Body::from("Invalid JSON"));
                        *bad.status_mut() = StatusCode::BAD_REQUEST;
                        return Ok::<_, hyper::Error>(bad);
                    }

                    (&Method::DELETE, p) if p.starts_with("/api/memory/") => {
                        // DELETE /api/memory/{key}
                        let key = &p["/api/memory/".len()..];
                        if key.is_empty() {
                            let mut bad = Response::new(Body::from("Missing key"));
                            *bad.status_mut() = StatusCode::BAD_REQUEST;
                            return Ok::<_, hyper::Error>(bad);
                        }

                        let ws_candidates = [
                            "/home/user/.openclaw",
                            "/home/user",
                            "/home/user/Project/zeroclaw",
                            "/home/user/goldclaw",
                        ];
                        let mut removed = false;
                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = goldclaw_memory::SqliteMemory::new(p) {
                                match mem.forget(key).await {
                                    Ok(ok) => { if ok { removed = true; break; } }
                                    Err(_) => continue,
                                }
                            }
                        }

                        let body = serde_json::json!({"ok": removed});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        return Ok::<_, hyper::Error>(res);
                    }

                    // fallthrough: keep existing /api/memories.json handler below
                    (&Method::GET, "/api/memories.json") => {
                        // Disk-first
                        let candidates = [
                            "/home/user/goldclaw/web/dist/api/memories.json",
                            "/mnt/c/Users/User/zero-ui/public/api/memories.json",
                            "/home/user/.openclaw/memories.json",
                        ];
                        for p in &candidates {
                            let pstr = resolve_repo_path(p);
                            let path = std::path::Path::new(&pstr);
                            if let Ok(m) = tokio::fs::metadata(path).await {
                                if m.is_file() {
                                    if let Ok(bytes) = tokio::fs::read(path).await {
                                        let mut res = Response::new(Body::from(bytes));
                                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                        return Ok::<_, hyper::Error>(res);
                                    }
                                }
                            }
                        }

                        // Try live memory backend (Sqlite / LanceDB via goldclaw-memory crate)
                        // Fallbacks: try several plausible workspace dirs; first non-empty result wins.
                        let mut items_json: Option<Vec<serde_json::Value>> = None;
                        let ws_candidates = [
                            "/home/user/.openclaw",
                            "/home/user",
                            "/home/user/Project/zeroclaw",
                            "/home/user/goldclaw",
                        ];

                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = goldclaw_memory::SqliteMemory::new(p) {
                                if let Ok(entries) = mem.recall("", 10, None, None, None).await {
                                    if !entries.is_empty() {
                                        let mapped: Vec<serde_json::Value> = entries
                                            .into_iter()
                                            .map(|e| {
                                                let distance = e.score;
                                                serde_json::json!({
                                                    "text_chunk": e.content,
                                                    "vector_id": e.id,
                                                    "distance": distance,
                                                    "timestamp": e.timestamp,
                                                    "namespace": e.namespace
                                                })
                                            })
                                            .collect();
                                        items_json = Some(mapped);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(items) = items_json {
                            let body = serde_json::json!({"memories": items});
                            let mut res = Response::new(Body::from(body.to_string()));
                            res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                            return Ok::<_, hyper::Error>(res);
                        }

                        // Final fallback: in-memory pinned fragments
                        match crate::memory_store::top_pinned(10).await {
                            items => {
                                let body = serde_json::json!({"memories": items});
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                return Ok::<_, hyper::Error>(res);
                            }
                        }
                    }

                    (&Method::POST, "/api/memories/delete") => {
                        // Delete a memory by vector_id (body JSON: {"vector_id":"..."})
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&whole) {
                            if let Some(id) = v.get("vector_id").and_then(|s| s.as_str()) {
                                // Try same workspace candidates as above
                                let ws_candidates = [
                                    "/home/user/.openclaw",
                                    "/home/user",
                                    "/home/user/Project/zeroclaw",
                                    "/home/user/goldclaw",
                                ];
                                let mut removed = false;
                                for ws in &ws_candidates {
                                    let pstr = (resolve_repo_path)(ws);
                                    let p = std::path::Path::new(&pstr);
                                    if let Ok(mem) = goldclaw_memory::SqliteMemory::new(p) {
                                        match mem.forget(id).await {
                                            Ok(ok) => { if ok { removed = true; break; } }
                                            Err(_) => continue,
                                        }
                                    }
                                }
                                let body = serde_json::json!({"ok": removed});
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                return Ok::<_, hyper::Error>(res);
                            }
                        }
                        let mut bad = Response::new(Body::from("Invalid JSON or missing vector_id"));
                        *bad.status_mut() = StatusCode::BAD_REQUEST;
                        return Ok::<_, hyper::Error>(bad);
                    }

                    (&Method::GET, "/api/admin/models/test.json") => {
                        // Disk-first
                        let candidates = [
                            "/home/user/goldclaw/web/dist/api/admin/models/test.json",
                            "/mnt/c/Users/User/zero-ui/public/api/admin/models/test.json",
                        ];
                        for p in &candidates {
                            let pstr = resolve_repo_path(p);
                            let path = std::path::Path::new(&pstr);
                            if let Ok(m) = tokio::fs::metadata(path).await {
                                if m.is_file() {
                                    if let Ok(bytes) = tokio::fs::read(path).await {
                                        let mut res = Response::new(Body::from(bytes));
                                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                        return Ok::<_, hyper::Error>(res);
                                    }
                                }
                            }
                        }

                        // Prefer HTTP-based Ollama API check using POST /api/show for model VRAM info
                        let client = reqwest::Client::new();
                        match client.post("http://127.0.0.1:11434/api/show").json(&serde_json::json!({"model":"gemma-4-uncensored"})).send().await {
                            Ok(resp) => {
                                if let Ok(json) = resp.json::<serde_json::Value>().await {
                                    let mut res = Response::new(Body::from(json.to_string()));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                    return Ok::<_, hyper::Error>(res);
                                } else {
                                    let body = serde_json::json!({"ok": false, "error": "invalid json from ollama"});
                                    let mut res = Response::new(Body::from(body.to_string()));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                            Err(_) => {
                                // Fallback to TCP preview probe
                                match tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await {
                                    Ok(mut s) => {
                                        let _ = s.write_all(b"GET /api/ps HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").await;
                                        let mut buf = vec![0u8; 4096];
                                        match s.read(&mut buf).await {
                                            Ok(n) => {
                                                let txt = String::from_utf8_lossy(&buf[..n]).to_string();
                                                let preview = txt.lines().take(20).collect::<Vec<_>>().join("\n");
                                                let body = serde_json::json!({"ok": true, "preview": preview});
                                                let mut res = Response::new(Body::from(body.to_string()));
                                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                                return Ok::<_, hyper::Error>(res);
                                            }
                                            Err(_) => {
                                                let body = serde_json::json!({"ok": false, "error": "no response"});
                                                let mut res = Response::new(Body::from(body.to_string()));
                                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                                return Ok::<_, hyper::Error>(res);
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        let body = serde_json::json!({"ok": false, "error": "connection failed"});
                                        let mut res = Response::new(Body::from(body.to_string()));
                                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                        return Ok::<_, hyper::Error>(res);
                                    }
                                }
                            }
                        }
                    }

                    _ => {
                        // If this is a GET request, try to serve static files from
                        // the built web output at /home/user/goldclaw/web/dist.
                        if req.method() == &Method::GET {
                            let base_pstr = resolve_repo_path("/home/user/goldclaw/web/dist");
                            let base = std::path::Path::new(&base_pstr);
                            let req_path = req.uri().path();
                            let rel = if req_path == "/" { "index.html" } else { req_path.trim_start_matches('/') };

                            // Try the exact path first
                            let fs_path = base.join(rel);
                            if let Ok(meta) = tokio::fs::metadata(&fs_path).await {
                                if meta.is_file() {
                                    if let Ok(bytes) = tokio::fs::read(&fs_path).await {
                                        let mut res = Response::new(Body::from(bytes));
                                        if let Some(ext) = fs_path.extension().and_then(|s| s.to_str()) {
                                            let ct = match ext {
                                                "html" => "text/html; charset=utf-8",
                                                "css" => "text/css; charset=utf-8",
                                                "js" => "application/javascript; charset=utf-8",
                                                "png" => "image/png",
                                                "jpg" | "jpeg" => "image/jpeg",
                                                "svg" => "image/svg+xml",
                                                "wasm" => "application/wasm",
                                                "json" => "application/json; charset=utf-8",
                                                "map" => "application/json; charset=utf-8",
                                                _ => "application/octet-stream",
                                            };
                                            res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static(ct));
                                        }
                                        return Ok(res);
                                    }
                                }
                            }

                            // Compatibility fallback: some SPA builds reference assets under "/_app/..."
                            // while the dist output places them at the repo root (e.g. "/assets/...").
                            // If the request starts with "_app/", try serving the file with that
                            // segment removed (drop the `_app` prefix).
                            if rel.starts_with("_app/") {
                                let without_prefix = &rel["_app/".len()..];
                                let alt_path = base.join(without_prefix);
                                if let Ok(meta) = tokio::fs::metadata(&alt_path).await {
                                    if meta.is_file() {
                                        if let Ok(bytes) = tokio::fs::read(&alt_path).await {
                                            let mut res = Response::new(Body::from(bytes));
                                            if let Some(ext) = alt_path.extension().and_then(|s| s.to_str()) {
                                                let ct = match ext {
                                                    "html" => "text/html; charset=utf-8",
                                                    "css" => "text/css; charset=utf-8",
                                                    "js" => "application/javascript; charset=utf-8",
                                                    "png" => "image/png",
                                                    "jpg" | "jpeg" => "image/jpeg",
                                                    "svg" => "image/svg+xml",
                                                    "wasm" => "application/wasm",
                                                    "json" => "application/json; charset=utf-8",
                                                    "map" => "application/json; charset=utf-8",
                                                    _ => "application/octet-stream",
                                                };
                                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static(ct));
                                            }
                                            return Ok(res);
                                        }
                                    }
                                }

                                // special-case legacy favicon reference
                                if rel == "_app/zeroclaw-trans.png" {
                                    let logo_path = base.join("logo.png");
                                    if let Ok(meta) = tokio::fs::metadata(&logo_path).await {
                                        if meta.is_file() {
                                            if let Ok(bytes) = tokio::fs::read(&logo_path).await {
                                                let mut res = Response::new(Body::from(bytes));
                                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("image/png"));
                                                return Ok(res);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let mut not_found = Response::default();
                        *not_found.status_mut() = StatusCode::NOT_FOUND;
                        Ok(not_found)
                    }
                }
            }
        });

        // Spawn a task to serve this connection
        tokio::spawn(async move {
            if let Err(err) = Http::new().serve_connection(stream, svc).await {
                log::error!("Gateway connection error: {:?}", err);
            }
        });
    }
}
