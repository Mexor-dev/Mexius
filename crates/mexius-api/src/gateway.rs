use hyper::{service::service_fn, Body, Request, Response, Method, StatusCode, server::conn::Http};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, broadcast, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::{SystemTime, UNIX_EPOCH};
use hyper::body::Bytes;
use serde::Deserialize;
use mexius_memory::MemoryCategory as GMemoryCategory;

const INDEX_HTML: &str = r#"<!doctype html>
<html lang=\"en\">
<head>
    <meta charset=\"utf-8\">
    <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">
    <title>Mexius Dashboard</title>
    <link rel=\"stylesheet\" href=\"/styles.css\">
    <style>body{margin:0;padding:0}</style>
</head>
<body>
    <div id=\"app\" class=\"app\">
        <header class=\"hdr\"><h1>Mexius</h1><div class=\"status\" id=\"status\">disconnected</div></header>
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

const APP_JS: &str = r#"document.addEventListener('DOMContentLoaded',()=>{const log=document.getElementById('log');const status=document.getElementById('status');const health=document.getElementById('health');const es=new EventSource('/logs');es.onopen=()=>{status.textContent='connected'};es.onerror=()=>{status.textContent='error'};es.onmessage=(e)=>{const d=document.createElement('div');d.textContent=e.data;log.appendChild(d);log.scrollTop=log.scrollHeight};async function refreshHealth(){try{const r=await fetch('/api/system/health');const j=await r.json();health.innerHTML=`<pre>${JSON.stringify(j,null,2)}</pre>`;if(j && j.external_ip){const ip=document.getElementById('external_ip');if(ip) ip.textContent=j.external_ip;}}catch(err){health.textContent='Health check failed: '+err}};refreshHealth();setInterval(refreshHealth,5000);document.getElementById('run').addEventListener('click',async ()=>{const v=document.getElementById('cmd').value;try{const res=await fetch('/api/command',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({intent:'run_tool:shell',content:v})});const txt=await res.text();const d=document.createElement('div');d.textContent='> '+v;log.appendChild(d);const o=document.createElement('div');o.textContent='<= '+txt;log.appendChild(o);log.scrollTop=log.scrollHeight}catch(err){const e=document.createElement('div');e.textContent='ERROR: '+err;log.appendChild(e)}});document.getElementById('cmd').addEventListener('keydown',e=>{if(e.key==='Enter'){document.getElementById('run').click();e.preventDefault()}});document.getElementById('reboot').addEventListener('click',async ()=>{if(!confirm('Restart Mexius service?')) return;try{const r=await fetch('/api/system/reboot',{method:'POST'});const t=await r.text();const e=document.createElement('div');e.textContent='Reboot: '+t;log.appendChild(e)}catch(err){const e=document.createElement('div');e.textContent='Reboot failed: '+err;log.appendChild(e)}});});"#;

#[derive(Deserialize)]
struct UiCommand {
    intent: Option<String>,
    content: Option<String>,
    id: Option<String>,
}

#[derive(Deserialize)]
struct OllamaPullRequest {
    model: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_try_serve_index_for_dashboard() {
        let td = tempdir().expect("tempdir");
        let root = td.path().to_str().unwrap().to_string();
        let dist = std::path::Path::new(&root).join("web").join("dist");
        tokio::fs::create_dir_all(&dist).await.unwrap();
        let index_path = dist.join("index.html");
        tokio::fs::write(&index_path, b"<html>TEST INDEX</html>").await.unwrap();

        let res = try_serve_static_file(&root, "/dashboard").await;
        assert!(res.is_some(), "expected index for /dashboard");
        let (bytes, ct) = res.unwrap();
        assert_eq!(ct, "text/html; charset=utf-8");
        assert!(String::from_utf8_lossy(&bytes).contains("TEST INDEX"));
    }

    #[tokio::test]
    async fn test_try_serve_app_asset_mapping() {
        let td = tempdir().expect("tempdir");
        let root = td.path().to_str().unwrap().to_string();
        let assets = std::path::Path::new(&root).join("web").join("dist").join("assets");
        tokio::fs::create_dir_all(&assets).await.unwrap();
        let asset_path = assets.join("index-BJsuCaeP.js");
        tokio::fs::write(&asset_path, b"console.log('ok')").await.unwrap();

        let req = "/_app/assets/index-BJsuCaeP.js";
        let res = try_serve_static_file(&root, req).await;
        assert!(res.is_some(), "expected asset for {}", req);
        let (bytes, ct) = res.unwrap();
        assert_eq!(ct, "application/javascript; charset=utf-8");
        assert!(String::from_utf8_lossy(&bytes).contains("console.log('ok')"));
    }
}

/// Run the Hyper gateway and serve embedded UI assets. The gateway receives
/// `tx` so it can forward UI commands into the Hermes channel, and `log_tx`
/// is used to stream server logs/events to connected UI clients over SSE.
pub async fn run(
    addr: SocketAddr,
    tx: mpsc::Sender<crate::hermes::Message>,
    log_tx: broadcast::Sender<String>,
    thought_tx: broadcast::Sender<String>,
    lattice_entity: crate::lattice::SharedEntity,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Ensure Mexius data directory exists (for LanceDB storage)
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let herma_path = std::path::Path::new(&home).join(".mexius");
    let lancedb_path = herma_path.join("lancedb_store");
    match tokio::fs::create_dir_all(&lancedb_path).await {
        Ok(()) => log::info!("Ensured Mexius data dir: {:?}", lancedb_path),
        Err(e) => log::warn!("Failed to create Mexius data dir {:?}: {}", lancedb_path, e),
    }

    // Inspect recent runtime log for known transient bridge errors and warn
    if let Ok(recent) = tokio::fs::read_to_string("/tmp/mexius.log").await {
        if recent.contains("IncompleteMessage") || recent.contains("ConnectionRefused") {
            log::warn!("Detected previous bridge errors in /tmp/mexius.log (IncompleteMessage/ConnectionRefused). Gateway will use retry/backoff for health probes.");
        }
    }

    // Repo root; allow override with MEXIUS_ROOT env var (useful after renaming repo)
    let repo_root = Arc::new(std::env::var("MEXIUS_ROOT").unwrap_or_else(|_| format!("{}/mexius", home)));
    // In-memory pairing state (code, optional token). Persisted to ~/.mexius/pairing.json
    // Default to paired for local development: initialize with a placeholder
    // code and a token so the gateway reports `paired: true` by default.
    let pairing_state: Arc<Mutex<Option<(String, Option<String>)>>> = Arc::new(Mutex::new(Some((
        "localdev".to_string(),
        Some("localdev-token".to_string()),
    ))));
    let pairing_file = lancedb_path.parent().unwrap_or(&herma_path).join("pairing.json");

    // ─── PIN Lock State ──────────────────────────────────────────────────────
    // Stores "salt_hex:hash_hex" or None if no PIN is configured.
    // Hash = SHA-256(salt_bytes ++ pin_bytes) stored as hex.
    let pin_file_path = herma_path.join("pin.json");
    let pin_initial: Option<String> = tokio::fs::read_to_string(&pin_file_path).await.ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("hash").and_then(|h| h.as_str()).map(|s| s.to_string()));
    let pin_state: Arc<tokio::sync::Mutex<Option<String>>> =
        Arc::new(tokio::sync::Mutex::new(pin_initial));

    // ─── Sovereignty State ───────────────────────────────────────────────────
    // Shared operational mode: Idle / Active / Dreaming / Nexus.
    let sovereignty_state = crate::sovereignty::new_shared_state();

    // ─── Nexus Broadcast Channel ─────────────────────────────────────────────
    // NexusAgentMessage events are broadcast to all connected /ws/nexus clients.
    let (nexus_tx, _nexus_rx_init) = crate::sovereignty::new_nexus_channel();
    let nexus_tx = Arc::new(nexus_tx);

    // ─── Model Registry ──────────────────────────────────────────────────────
    let model_registry = crate::model_registry::new_registry();
    {
        let loaded = crate::model_registry::load_registry(&repo_root).await;
        let mut reg = model_registry.write().await;
        *reg = loaded;
    }

    // ─── Dream Worker ────────────────────────────────────────────────────────
    {
        let dream_state = sovereignty_state.clone();
        let dream_root = repo_root.as_ref().clone();
        let dream_log = log_tx.clone();
        tokio::spawn(async move {
            crate::sovereignty::run_dream_worker(dream_state, dream_root, dream_log).await;
        });
    }

    // Make a clonable, thread-safe function object for resolving paths so it
    // can be moved into service closures without ownership issues.
    let resolve_repo_path: Arc<dyn Fn(&str) -> String + Send + Sync> = {
        let repo_root = repo_root.clone();
        Arc::new(move |p: &str| {
            if p.starts_with("/home/user/mexius") {
                p.replacen("/home/user/mexius", repo_root.as_str(), 1)
            } else {
                p.to_string()
            }
        })
    };

    // Helper to ensure permissive CORS headers are present on responses so
    // browsers (Windows host) can connect to the WSL backend without WebSocket
    // or fetch CORS failures.
    fn add_cors_headers(res: &mut Response<Body>) {
        use hyper::header::{ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_HEADERS};
        res.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, hyper::header::HeaderValue::from_static("*"));
        res.headers_mut().insert(ACCESS_CONTROL_ALLOW_METHODS, hyper::header::HeaderValue::from_static("GET, POST, PUT, OPTIONS"));
        res.headers_mut().insert(ACCESS_CONTROL_ALLOW_HEADERS, hyper::header::HeaderValue::from_static("Content-Type, Authorization, Upgrade"));
        // NOTE: credentials must NOT be "true" when Allow-Origin is "*" (RFC 6454).
        // Removed ACCESS_CONTROL_ALLOW_CREDENTIALS to comply with the CORS spec.
    }

    // Helper: try to locate and read a static file for a given request path.
    // Handles exact files under `${MEXIUS_ROOT}/web/dist`, the `_app/` prefix
    // compatibility mapping, a legacy favicon mapping, and SPA index fallback.
    async fn try_serve_static_file(herma_root: &str, req_path: &str) -> Option<(Vec<u8>, &'static str)> {
        let base_pstr = format!("{}/web/dist", herma_root);
        let base = std::path::Path::new(&base_pstr);
        let rel = if req_path == "/" { "index.html" } else { req_path.trim_start_matches('/') };

        // Try exact file first
        let fs_path = base.join(rel);
        if let Ok(meta) = tokio::fs::metadata(&fs_path).await {
            if meta.is_file() {
                if let Ok(bytes) = tokio::fs::read(&fs_path).await {
                    let ct = match fs_path.extension().and_then(|s| s.to_str()) {
                        Some("html") => "text/html; charset=utf-8",
                        Some("css") => "text/css; charset=utf-8",
                        Some("js") => "application/javascript; charset=utf-8",
                        Some("png") => "image/png",
                        Some("jpg") | Some("jpeg") => "image/jpeg",
                        Some("svg") => "image/svg+xml",
                        Some("wasm") => "application/wasm",
                        Some("json") => "application/json; charset=utf-8",
                        Some("map") => "application/json; charset=utf-8",
                        _ => "application/octet-stream",
                    };
                    log::debug!("Static: serving {} for request {}", fs_path.display(), req_path);
                    return Some((bytes, ct));
                }
            }
        }

        // Support `_app/` prefix mapping used by some SPA builds
        if rel.starts_with("_app/") {
            let without_prefix = &rel["_app/".len()..];
            let alt_path = base.join(without_prefix);
            if let Ok(meta) = tokio::fs::metadata(&alt_path).await {
                if meta.is_file() {
                    if let Ok(bytes) = tokio::fs::read(&alt_path).await {
                        let ct = match alt_path.extension().and_then(|s| s.to_str()) {
                            Some("html") => "text/html; charset=utf-8",
                            Some("css") => "text/css; charset=utf-8",
                            Some("js") => "application/javascript; charset=utf-8",
                            Some("png") => "image/png",
                            Some("jpg") | Some("jpeg") => "image/jpeg",
                            Some("svg") => "image/svg+xml",
                            Some("wasm") => "application/wasm",
                            Some("json") => "application/json; charset=utf-8",
                            Some("map") => "application/json; charset=utf-8",
                            _ => "application/octet-stream",
                        };
                        log::debug!("Static: serving {} for request {}", alt_path.display(), req_path);
                        return Some((bytes, ct));
                    }
                }
            }

            // Legacy favicon alias
            if rel == "_app/zeroclaw-trans.png" {
                let logo_path = base.join("logo.png");
                if let Ok(meta) = tokio::fs::metadata(&logo_path).await {
                    if meta.is_file() {
                        if let Ok(bytes) = tokio::fs::read(&logo_path).await {
                            log::debug!("Static: serving {} for request {}", logo_path.display(), req_path);
                            return Some((bytes, "image/png"));
                        }
                    }
                }
            }
        }

        // SPA fallback: if index.html exists, serve it for client-side routes
        let index_path = base.join("index.html");
        if let Ok(meta) = tokio::fs::metadata(&index_path).await {
            if meta.is_file() {
                if let Ok(bytes) = tokio::fs::read(&index_path).await {
                    log::debug!("Static: serving index.html for request {}", req_path);
                    return Some((bytes, "text/html; charset=utf-8"));
                }
            }
        }

        None
    }

    let listener = TcpListener::bind(addr).await?;
    log::info!("Gateway listening on http://{}", addr);

    loop {
        let (stream, _peer) = listener.accept().await?;

        // Per-connection clones
        let tx_conn = tx.clone();
        let log_tx_conn = log_tx.clone();
        let thought_tx_conn = thought_tx.clone();
        let resolve_repo_path_conn = resolve_repo_path.clone();
        // Provide the MEXIUS_ROOT value to connections so static files can be
        // served from an absolute path: ${MEXIUS_ROOT}/web/dist
        let herma_root_conn = repo_root.clone();
        let pairing_state_conn = pairing_state.clone();
        let pairing_file_conn = pairing_file.clone();
        let lattice_entity_conn = lattice_entity.clone();
        let sovereignty_state_conn = sovereignty_state.clone();
        let nexus_tx_conn = nexus_tx.clone();
        let model_registry_conn = model_registry.clone();
        let pin_state_conn = pin_state.clone();
        let pin_file_conn = pin_file_path.clone();

        let pairing_state_value = pairing_state_conn.clone();
        let pairing_file_value = pairing_file_conn.clone();
        let lattice_entity_value = lattice_entity_conn.clone();
        let sovereignty_state_value = sovereignty_state_conn.clone();
        let nexus_tx_value = nexus_tx_conn.clone();
        let model_registry_value = model_registry_conn.clone();
        let pin_state_value = pin_state_conn.clone();
        let pin_file_value = pin_file_conn.clone();

        let svc = service_fn(move |req: Request<Body>| {
            let tx_req = tx_conn.clone();
            let log_tx_req = log_tx_conn.clone();
            let thought_tx_req = thought_tx_conn.clone();
            let resolve_repo_path = resolve_repo_path_conn.clone();
            let herma_root = herma_root_conn.clone();
            let pairing_state_req = pairing_state_value.clone();
            let pairing_file_req = pairing_file_value.clone();
            let lattice_entity_req = lattice_entity_value.clone();
            let sovereignty_state_req = sovereignty_state_value.clone();
            let nexus_tx_req = nexus_tx_value.clone();
            let model_registry_req = model_registry_value.clone();
            let pin_state_req = pin_state_value.clone();
            let pin_file_req = pin_file_value.clone();
            async move {
                        fn parse_top_level_string_value(toml: &str, key: &str) -> Option<String> {
                            let mut in_section = false;
                            for raw_line in toml.lines() {
                                let line = raw_line.trim();
                                if line.is_empty() || line.starts_with('#') {
                                    continue;
                                }
                                if line.starts_with('[') && line.ends_with(']') {
                                    in_section = true;
                                    continue;
                                }
                                if in_section {
                                    continue;
                                }
                                if let Some((lhs, rhs)) = line.split_once('=') {
                                    if lhs.trim() == key {
                                        return Some(rhs.trim().trim_matches('"').to_string());
                                    }
                                }
                            }
                            None
                        }

                        fn parse_top_level_number_value(toml: &str, key: &str) -> Option<f64> {
                            let mut in_section = false;
                            for raw_line in toml.lines() {
                                let line = raw_line.trim();
                                if line.is_empty() || line.starts_with('#') {
                                    continue;
                                }
                                if line.starts_with('[') && line.ends_with(']') {
                                    in_section = true;
                                    continue;
                                }
                                if in_section {
                                    continue;
                                }
                                if let Some((lhs, rhs)) = line.split_once('=') {
                                    if lhs.trim() == key {
                                        return rhs.trim().parse::<f64>().ok();
                                    }
                                }
                            }
                            None
                        }

                        async fn read_saved_provider_config(herma_root: &str) -> (Option<String>, Option<String>, Option<f64>) {
                            let config_path = format!("{}/config.toml", herma_root);
                            match tokio::fs::read_to_string(std::path::Path::new(&config_path)).await {
                                Ok(content) => (
                                    parse_top_level_string_value(&content, "default_provider"),
                                    parse_top_level_string_value(&content, "default_model"),
                                    parse_top_level_number_value(&content, "default_temperature"),
                                ),
                                Err(_) => (None, None, None),
                            }
                        }

                        // TTL cache for Ollama model list — avoids hammering Ollama on every
                        // /api/status and /api/ollama/models request. Cache expires after 10s.
                        use std::sync::OnceLock;
                        use std::sync::Mutex as StdMutex;
                        static OLLAMA_MODEL_CACHE: OnceLock<StdMutex<(Vec<String>, std::time::Instant)>> = OnceLock::new();

                        async fn fetch_ollama_model_names() -> Result<Vec<String>, String> {
                            const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(10);
                            let cache = OLLAMA_MODEL_CACHE.get_or_init(|| {
                                StdMutex::new((Vec::new(), std::time::Instant::now() - CACHE_TTL - std::time::Duration::from_secs(1)))
                            });
                            // Check cache first (hold lock briefly)
                            {
                                if let Ok(guard) = cache.lock() {
                                    if guard.0.is_empty() == false && guard.1.elapsed() < CACHE_TTL {
                                        return Ok(guard.0.clone());
                                    }
                                }
                            }
                            // Cache miss — fetch from Ollama
                            let client = reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(3))
                                .build()
                                .map_err(|e| format!("client build failed: {}", e))?;

                            let resp = client
                                .get("http://127.0.0.1:11434/api/tags")
                                .send()
                                .await
                                .map_err(|e| format!("ollama request failed: {}", e))?;

                            let json = resp
                                .json::<serde_json::Value>()
                                .await
                                .map_err(|e| format!("invalid ollama json: {}", e))?;

                            let names = json
                                .get("models")
                                .and_then(|v| v.as_array())
                                .map(|models| {
                                    models
                                        .iter()
                                        .filter_map(|entry| entry.get("name").and_then(|v| v.as_str()))
                                        .map(|name| name.to_string())
                                        .collect::<Vec<String>>()
                                })
                                .unwrap_or_default();

                            // Update cache
                            if let Ok(mut guard) = cache.lock() {
                                *guard = (names.clone(), std::time::Instant::now());
                            }
                            Ok(names)
                        }

                // Verbose logging: print every incoming request method, path and query
                let q = req.uri().query().map(|s| format!("?{}", s)).unwrap_or_default();
                log::debug!("Incoming request: {} {}{}", req.method(), req.uri().path(), q);

                // Respond to CORS preflight requests early with permissive headers
                if req.method() == &Method::OPTIONS {
                    let mut res = Response::new(Body::empty());
                    add_cors_headers(&mut res);
                    return Ok::<_, hyper::Error>(res);
                }

                // Normalize incoming API prefix to support both `/api/*` and `/api/v1/*`
                let mut normalized_path = req.uri().path().to_string();
                if normalized_path.starts_with("/api/v1/") {
                    normalized_path = normalized_path.replacen("/api/v1", "/api", 1);
                }

                // Map /api/v1/* to /api/* for backward compatibility only.
                // NOTE: /health and /api/cost now have dedicated handlers below
                // with correct response shapes for the SPA.

                match (req.method(), normalized_path.as_str()) {
                    // Public pairing code endpoint used by the SPA
                    (&Method::GET, "/pair/code") => {
                        let ps = pairing_state_req.clone();
                        let guard = ps.lock().await;
                        // Only require pairing if there is no token present.
                        let (pairing_code, pairing_required) = if let Some((code, token_opt)) = &*guard {
                            let req = token_opt.is_none();
                            (serde_json::Value::String(code.clone()), serde_json::Value::Bool(req))
                        } else {
                            (serde_json::Value::Null, serde_json::Value::Bool(true))
                        };
                        let body = serde_json::json!({"pairing_code": pairing_code, "pairing_required": pairing_required});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    // POST /pair with header X-Pairing-Code - exchange for token
                    (&Method::POST, "/pair") => {
                        // Expect header X-Pairing-Code
                        let code_header = req.headers().get("X-Pairing-Code");
                        if code_header.is_none() {
                            let mut bad = Response::new(Body::from("Missing X-Pairing-Code header"));
                            *bad.status_mut() = StatusCode::BAD_REQUEST;
                            add_cors_headers(&mut bad);
                            return Ok::<_, hyper::Error>(bad);
                        }
                        let code_str = match code_header.and_then(|h| h.to_str().ok()) {
                            Some(s) => s.to_string(),
                            None => {
                                let mut bad = Response::new(Body::from("Invalid X-Pairing-Code header"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                add_cors_headers(&mut bad);
                                return Ok::<_, hyper::Error>(bad);
                            }
                        };

                        // Validate against pairing state
                        let mut guard = pairing_state_req.lock().await;
                        if let Some((stored_code, _token)) = guard.as_ref() {
                            if stored_code != &code_str {
                                let mut unauthorized = Response::new(Body::from("Invalid pairing code"));
                                *unauthorized.status_mut() = StatusCode::UNAUTHORIZED;
                                add_cors_headers(&mut unauthorized);
                                return Ok::<_, hyper::Error>(unauthorized);
                            }

                            // Code matches: generate token
                            let token = uuid::Uuid::new_v4().to_string();
                            let stored_clone = stored_code.clone();
                            *guard = Some((stored_clone.clone(), Some(token.clone())));

                            // Persist pairing file
                            let pairing_json = serde_json::json!({"pairing_code": stored_clone, "token": token});
                            if let Err(e) = tokio::fs::write(&pairing_file_req, pairing_json.to_string()).await {
                                log::warn!("Failed to persist pairing file: {}", e);
                            }

                            let mut res = Response::new(Body::from(serde_json::json!({"token": guard.as_ref().and_then(|(_, t)| t.clone()).unwrap_or_default()}).to_string()));
                            res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                            add_cors_headers(&mut res);
                            return Ok::<_, hyper::Error>(res);
                        } else {
                            let mut unauthorized = Response::new(Body::from("No active pairing code"));
                            *unauthorized.status_mut() = StatusCode::UNAUTHORIZED;
                            add_cors_headers(&mut unauthorized);
                            return Ok::<_, hyper::Error>(unauthorized);
                        }
                    }

                    // Initiate pairing: generate a 6-digit code and return it
                    (&Method::POST, "/api/pairing/initiate") => {
                        // Generate simple 6-digit numeric code based on time
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
                        let code = format!("{:06}", (now % 1_000_000));
                        let mut guard = pairing_state_req.lock().await;
                        *guard = Some((code.clone(), None));

                        // Persist pairing file (token empty until exchanged)
                        let pairing_json = serde_json::json!({"pairing_code": code});
                        if let Err(e) = tokio::fs::write(&pairing_file_req, pairing_json.to_string()).await {
                            log::warn!("Failed to persist pairing file: {}", e);
                        }

                        let mut res = Response::new(Body::from(serde_json::json!({"pairing_code": code}).to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    // ── PIN Lock Routes ──────────────────────────────────────
                    // SHA-256(salt_hex + pin) stored as "salt_hex:hash_hex"

                    (&Method::OPTIONS, p) if p == "/api/pin/status" || p == "/api/pin/set" || p == "/api/pin/verify" || p == "/api/pin/reset" => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::GET, "/api/pin/status") => {
                        let guard = pin_state_req.lock().await;
                        let body = serde_json::json!({ "has_pin": guard.is_some() });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/set") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let parsed: serde_json::Value = serde_json::from_slice(&whole).unwrap_or_default();
                        let pin = parsed.get("pin").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        // Validate: must be 4 digits
                        if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
                            let mut bad = Response::new(Body::from(r#"{"error":"PIN must be exactly 4 digits"}"#));
                            *bad.status_mut() = StatusCode::BAD_REQUEST;
                            add_cors_headers(&mut bad);
                            return Ok::<_, hyper::Error>(bad);
                        }

                        let mut guard = pin_state_req.lock().await;
                        if guard.is_some() {
                            let mut conflict = Response::new(Body::from(r#"{"error":"PIN already set. Use reset first."}"#));
                            *conflict.status_mut() = StatusCode::CONFLICT;
                            add_cors_headers(&mut conflict);
                            return Ok::<_, hyper::Error>(conflict);
                        }

                        // Generate salt (16 random bytes via ring)
                        use ring::rand::{SecureRandom, SystemRandom};
                        let rng = SystemRandom::new();
                        let mut salt = [0u8; 16];
                        if rng.fill(&mut salt).is_err() {
                            // Fallback salt from timestamp
                            let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
                            let ts_bytes = ts.to_le_bytes();
                            for (i, b) in ts_bytes.iter().enumerate() { salt[i % 16] ^= b; }
                        }
                        let salt_hex: String = salt.iter().map(|b| format!("{:02x}", b)).collect();
                        let hash_input = format!("{}{}", salt_hex, pin);
                        let hash_bytes = ring::digest::digest(&ring::digest::SHA256, hash_input.as_bytes());
                        let hash_hex: String = hash_bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect();
                        let stored = format!("{}:{}", salt_hex, hash_hex);

                        *guard = Some(stored.clone());
                        let pin_json = serde_json::json!({ "hash": stored });
                        if let Err(e) = tokio::fs::write(&pin_file_req, pin_json.to_string()).await {
                            log::warn!("Failed to persist pin file: {}", e);
                        }

                        let mut res = Response::new(Body::from(r#"{"ok":true}"#));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/verify") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let parsed: serde_json::Value = serde_json::from_slice(&whole).unwrap_or_default();
                        let pin = parsed.get("pin").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        let guard = pin_state_req.lock().await;
                        let ok = if let Some(stored) = guard.as_ref() {
                            if let Some((salt_hex, hash_hex)) = stored.split_once(':') {
                                let hash_input = format!("{}{}", salt_hex, pin);
                                let computed = ring::digest::digest(&ring::digest::SHA256, hash_input.as_bytes());
                                let computed_hex: String = computed.as_ref().iter().map(|b| format!("{:02x}", b)).collect();
                                computed_hex == hash_hex
                            } else { false }
                        } else {
                            true // No PIN set — always ok
                        };

                        let body = serde_json::json!({ "ok": ok });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/reset") => {
                        // Requires valid Bearer token (existing auth)
                        let auth_ok = {
                            let auth_header = req.headers().get(hyper::header::AUTHORIZATION);
                            auth_header.and_then(|h| h.to_str().ok())
                                .map(|v| !v.is_empty())
                                .unwrap_or(false)
                        };
                        if !auth_ok {
                            let mut unauth = Response::new(Body::from(r#"{"error":"Unauthorized"}"#));
                            *unauth.status_mut() = StatusCode::UNAUTHORIZED;
                            add_cors_headers(&mut unauth);
                            return Ok::<_, hyper::Error>(unauth);
                        }
                        let mut guard = pin_state_req.lock().await;
                        *guard = None;
                        if let Err(e) = tokio::fs::remove_file(&pin_file_req).await {
                            log::debug!("Pin file remove: {}", e);
                        }
                        let mut res = Response::new(Body::from(r#"{"ok":true}"#));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }


                    // ── PIN Lock Routes ──────────────────────────────────────
                    // SHA-256(salt_hex + pin) stored as "salt_hex:hash_hex"

                    (&Method::OPTIONS, p) if p == "/api/pin/status" || p == "/api/pin/set" || p == "/api/pin/verify" || p == "/api/pin/reset" => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::GET, "/api/pin/status") => {
                        let guard = pin_state_req.lock().await;
                        let body = serde_json::json!({ "has_pin": guard.is_some() });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/set") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let parsed: serde_json::Value = serde_json::from_slice(&whole).unwrap_or_default();
                        let pin = parsed.get("pin").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        // Validate: must be 4 digits
                        if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
                            let mut bad = Response::new(Body::from(r#"{"error":"PIN must be exactly 4 digits"}"#));
                            *bad.status_mut() = StatusCode::BAD_REQUEST;
                            add_cors_headers(&mut bad);
                            return Ok::<_, hyper::Error>(bad);
                        }

                        let mut guard = pin_state_req.lock().await;
                        if guard.is_some() {
                            let mut conflict = Response::new(Body::from(r#"{"error":"PIN already set. Use reset first."}"#));
                            *conflict.status_mut() = StatusCode::CONFLICT;
                            add_cors_headers(&mut conflict);
                            return Ok::<_, hyper::Error>(conflict);
                        }

                        // Generate salt (16 random bytes via ring)
                        use ring::rand::{SecureRandom, SystemRandom};
                        let rng = SystemRandom::new();
                        let mut salt = [0u8; 16];
                        if rng.fill(&mut salt).is_err() {
                            // Fallback salt from timestamp
                            let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
                            let ts_bytes = ts.to_le_bytes();
                            for (i, b) in ts_bytes.iter().enumerate() { salt[i % 16] ^= b; }
                        }
                        let salt_hex: String = salt.iter().map(|b| format!("{:02x}", b)).collect();
                        let hash_input = format!("{}{}", salt_hex, pin);
                        let hash_bytes = ring::digest::digest(&ring::digest::SHA256, hash_input.as_bytes());
                        let hash_hex: String = hash_bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect();
                        let stored = format!("{}:{}", salt_hex, hash_hex);

                        *guard = Some(stored.clone());
                        let pin_json = serde_json::json!({ "hash": stored });
                        if let Err(e) = tokio::fs::write(&pin_file_req, pin_json.to_string()).await {
                            log::warn!("Failed to persist pin file: {}", e);
                        }

                        let mut res = Response::new(Body::from(r#"{"ok":true}"#));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/verify") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let parsed: serde_json::Value = serde_json::from_slice(&whole).unwrap_or_default();
                        let pin = parsed.get("pin").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        let guard = pin_state_req.lock().await;
                        let ok = if let Some(stored) = guard.as_ref() {
                            if let Some((salt_hex, hash_hex)) = stored.split_once(':') {
                                let hash_input = format!("{}{}", salt_hex, pin);
                                let computed = ring::digest::digest(&ring::digest::SHA256, hash_input.as_bytes());
                                let computed_hex: String = computed.as_ref().iter().map(|b| format!("{:02x}", b)).collect();
                                computed_hex == hash_hex
                            } else { false }
                        } else {
                            true // No PIN set — always ok
                        };

                        let body = serde_json::json!({ "ok": ok });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    (&Method::POST, "/api/pin/reset") => {
                        // Requires valid Bearer token (existing auth)
                        let auth_ok = {
                            let auth_header = req.headers().get(hyper::header::AUTHORIZATION);
                            auth_header.and_then(|h| h.to_str().ok())
                                .map(|v| !v.is_empty())
                                .unwrap_or(false)
                        };
                        if !auth_ok {
                            let mut unauth = Response::new(Body::from(r#"{"error":"Unauthorized"}"#));
                            *unauth.status_mut() = StatusCode::UNAUTHORIZED;
                            add_cors_headers(&mut unauth);
                            return Ok::<_, hyper::Error>(unauth);
                        }
                        let mut guard = pin_state_req.lock().await;
                        *guard = None;
                        if let Err(e) = tokio::fs::remove_file(&pin_file_req).await {
                            log::debug!("Pin file remove: {}", e);
                        }
                        let mut res = Response::new(Body::from(r#"{"ok":true}"#));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    // Explicit SPA route mappings for client-side dashboard routes.
                    (&Method::GET, p) if p == "/dashboard" || p.starts_with("/dashboard/") ||
                                       p == "/tools" || p.starts_with("/tools/") ||
                                       p == "/cron" || p.starts_with("/cron/") ||
                                       p == "/integrations" || p.starts_with("/integrations/") => {
                        // Serve index.html from ${MEXIUS_ROOT}/web/dist if present,
                        // otherwise fall back to the embedded index.
                        let index_str = format!("{}/web/dist/index.html", &*herma_root);
                        let index_path = std::path::Path::new(&index_str);
                        if let Ok(meta) = tokio::fs::metadata(index_path).await {
                            if meta.is_file() {
                                if let Ok(bytes) = tokio::fs::read(index_path).await {
                                    let mut res = Response::new(Body::from(bytes));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/html; charset=utf-8"));
                                    add_cors_headers(&mut res);
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }

                        // Fallback to embedded index
                        Ok::<_, hyper::Error>(Response::new(Body::from(INDEX_HTML)))
                    }
                    (&Method::GET, "/") => {
                        // Serve index.html from ${MEXIUS_ROOT}/web/dist if present,
                        // otherwise fall back to the embedded index.
                        let index_str = format!("{}/web/dist/index.html", &*herma_root);
                        let index_path = std::path::Path::new(&index_str);
                        if let Ok(meta) = tokio::fs::metadata(index_path).await {
                            if meta.is_file() {
                                if let Ok(bytes) = tokio::fs::read(index_path).await {
                                    let mut res = Response::new(Body::from(bytes));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/html; charset=utf-8"));
                                    add_cors_headers(&mut res);
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }

                        // Fallback to embedded index
                        Ok::<_, hyper::Error>(Response::new(Body::from(INDEX_HTML)))
                    }
                    (&Method::GET, "/styles.css") => {
                        // Prefer disk-built stylesheet if present
                        let styles_str = format!("{}/web/dist/styles.css", &*herma_root);
                        let styles_path = std::path::Path::new(&styles_str);
                        if let Ok(meta) = tokio::fs::metadata(styles_path).await {
                            if meta.is_file() {
                                if let Ok(bytes) = tokio::fs::read(styles_path).await {
                                    let mut res = Response::new(Body::from(bytes));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/css; charset=utf-8"));
                                    add_cors_headers(&mut res);
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }
                        let mut res = Response::new(Body::from(STYLES_CSS));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/css; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }
                    (&Method::GET, "/app.js") => {
                        // Prefer disk-built app bundle if present
                        let app_str = format!("{}/web/dist/app.js", &*herma_root);
                        let app_path = std::path::Path::new(&app_str);
                        if let Ok(meta) = tokio::fs::metadata(app_path).await {
                            if meta.is_file() {
                                if let Ok(bytes) = tokio::fs::read(app_path).await {
                                    let mut res = Response::new(Body::from(bytes));
                                    res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/javascript; charset=utf-8"));
                                    add_cors_headers(&mut res);
                                    return Ok::<_, hyper::Error>(res);
                                }
                            }
                        }
                        let mut res = Response::new(Body::from(APP_JS));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/javascript; charset=utf-8"));
                        add_cors_headers(&mut res);
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
                        add_cors_headers(&mut res);
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
                        add_cors_headers(&mut res);
                        Ok(res)
                    }
                    (&Method::GET, "/api/system/health") => {
                        // Health check: ollama socket, hermes (local), memory, external IP
                        let mut health = serde_json::Map::new();

                        // Check Ollama and probe model VRAM state via HTTP /api/show with retries
                        let client = reqwest::Client::new();
                        let mut vram_loaded = false;
                        let mut model_name = serde_json::Value::String("".to_string());
                        let mut ollama_ok = false;
                        let mut last_err: Option<String> = None;

                        for attempt in 1..=3 {
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
                                    ollama_ok = true;
                                    break;
                                }
                                Err(e) => {
                                    last_err = Some(format!("{}", e));
                                    log::warn!("Ollama probe attempt {} failed: {}", attempt, last_err.as_ref().unwrap_or(&"unknown".to_string()));
                                    if attempt < 3 {
                                        let backoff = std::time::Duration::from_millis(200 * attempt as u64);
                                        tokio::time::sleep(backoff).await;
                                        continue;
                                    }
                                }
                            }
                        }

                        if ollama_ok {
                            health.insert("ollama".to_string(), serde_json::Value::Bool(true));
                        } else {
                            // Final fallback: TCP connect indicator
                            let tcp_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
                            health.insert("ollama".to_string(), serde_json::Value::Bool(tcp_ok));
                            if !tcp_ok {
                                if let Some(e) = last_err { log::warn!("Ollama health probe final failure: {}", e); }
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
                        add_cors_headers(&mut res);
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
                                .arg("mexius")
                                .output()
                                .await
                            {
                                Ok(o) => format!("status: {}", o.status),
                                Err(e) => format!("error: {}", e),
                            }
                        } else {
                            // Non-systemd fallback: try a best-effort pkill + nohup spawn
                            let bin = if std::path::Path::new("/usr/local/bin/mexius").exists() {
                                "/usr/local/bin/mexius".to_string()
                            } else {
                                // repo-local candidate
                                "/home/user/mexius/target/release/mexius".to_string()
                            };
                            let cmd = format!(
                                "pkill -f mexius || true; nohup {} gateway > /home/user/mexius/run_logs/gateway.log 2>&1 &",
                                bin
                            );
                            // Use non-blocking std::process spawn so reboot does not hang
                            match std::process::Command::new("sh").arg("-c").arg(cmd).spawn() {
                                Ok(_) => format!("fallback: spawned"),
                                Err(e) => format!("fallback error: {}", e),
                            }
                        };
                        let mut res = Response::new(Body::from(out));
                        add_cors_headers(&mut res);
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
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(_) => {
                                let mut bad = Response::new(Body::from("Invalid JSON"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                add_cors_headers(&mut bad);
                                Ok(bad)
                            }
                        }
                    }

                    // Serve disk-backed API JSON if present (dist/api or dev public/api),
                    // otherwise return dynamic, runtime-backed responses.
                    (&Method::GET, "/api/doctor.json") => {
                        // Try disk locations first
                        let candidates = [
                            "/home/user/mexius/web/dist/api/doctor.json",
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

                    (&Method::GET, "/api/status") => {
                        // Minimal status endpoint used by the frontend on startup.
                        let uptime_seconds = match tokio::fs::read_to_string("/proc/uptime").await {
                            Ok(s) => s.split_whitespace().next().and_then(|sec| sec.parse::<f64>().ok()).unwrap_or(0.0),
                            Err(_) => 0.0,
                        };

                        let (saved_provider, saved_model, saved_temperature) = read_saved_provider_config(&herma_root).await;
                        let detected_ollama_models = fetch_ollama_model_names().await.unwrap_or_default();
                        let provider = saved_provider.unwrap_or_else(|| {
                            if detected_ollama_models.is_empty() {
                                "openrouter".to_string()
                            } else {
                                "ollama".to_string()
                            }
                        });
                        let model = if let Some(model) = saved_model {
                            model
                        } else if provider == "ollama" {
                            detected_ollama_models.first().cloned().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let temperature = saved_temperature.unwrap_or(0.7);

                        // For local development, always report paired to bypass pairing gate.
                        let paired = true;

                        let status_json = serde_json::json!({
                            "provider": provider,
                            "model": model,
                            "temperature": temperature,
                            "uptime_seconds": uptime_seconds,
                            "gateway_port": 42617,
                            "locale": "en",
                            "memory_backend": "lancedb",
                            "paired": paired,
                            "channels": serde_json::json!({}),
                            "health": serde_json::json!({
                                "pid": std::process::id(),
                                "updated_at": "",
                                "uptime_seconds": uptime_seconds,
                                "components": serde_json::json!({})
                            })
                        });

                        let mut res = Response::new(Body::from(status_json.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        return Ok::<_, hyper::Error>(res);
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/ollama/models — dynamic local model discovery for Config UI
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/ollama/models") => {
                        match fetch_ollama_model_names().await {
                            Ok(models) => {
                                let body = serde_json::json!({
                                    "provider": "ollama",
                                    "reachable": true,
                                    "models": models,
                                });
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(err) => {
                                let body = serde_json::json!({
                                    "provider": "ollama",
                                    "reachable": false,
                                    "models": [],
                                    "error": err,
                                });
                                let mut res = Response::new(Body::from(body.to_string()));
                                *res.status_mut() = StatusCode::OK;
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                        }
                    }

                    // removed: /api/audit.json handled above as SSE stream

                    // ------------------------------------------------------------------
                    // /health  — public pairing gate check (SPA startup)
                    // ------------------------------------------------------------------
                    (&Method::GET, "/health") => {
                        let body = serde_json::json!({"require_pairing": false, "paired": true});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/health — HealthSnapshot (used by dashboard widgets)
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/health") => {
                        let uptime = match tokio::fs::read_to_string("/proc/uptime").await {
                            Ok(s) => s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
                            Err(_) => 0.0,
                        };
                        let body = serde_json::json!({
                            "pid": std::process::id(),
                            "updated_at": "",
                            "uptime_seconds": uptime,
                            "components": {}
                        });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/cost — CostSummary (all zeros until billing tracking is added)
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/cost") => {
                        let body = serde_json::json!({
                            "session_cost_usd": 0.0_f64,
                            "daily_cost_usd": 0.0_f64,
                            "monthly_cost_usd": 0.0_f64,
                            "total_tokens": 0,
                            "request_count": 0,
                            "by_model": {}
                        });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/tools — registered tool specs
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/tools") => {
                        let body = serde_json::json!([]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/cron — cron job list + settings
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/cron") => {
                        let body = serde_json::json!([]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::POST, "/api/cron") => {
                        let mut res = Response::new(Body::from("{\"error\":\"Cron scheduling not implemented\"}"));
                        *res.status_mut() = StatusCode::NOT_IMPLEMENTED;
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::GET, "/api/cron/settings") => {
                        let body = serde_json::json!({
                            "enabled": false,
                            "catch_up_on_startup": false,
                            "max_run_history": 50
                        });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::PATCH, "/api/cron/settings") => {
                        let body = serde_json::json!({"enabled":false,"catch_up_on_startup":false,"max_run_history":50,"status":"ok"});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/integrations
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/integrations") => {
                        let camofox_url = std::env::var("CAMOFOX_URL").unwrap_or_default();
                        let camofox_enabled = !camofox_url.trim().is_empty();
                        let camofox_status = if camofox_enabled { "Active" } else { "Available" };

                        let body = serde_json::json!([
                            {"name":"Ollama","description":"Local LLM inference via Ollama","category":"AI","status":"Active"},
                            {"name":"Memory","description":"LanceDB vector memory store","category":"Storage","status":"Active"},
                            {"name":"WebSearch","description":"Web search integration (DuckDuckGo / Camofox provider)","category":"Tools","status":"Active"},
                            {"name":"Camofox","description":"Local anti-detection browser integration via CAMOFOX_URL","category":"Browser","status": camofox_status},
                            {"name":"Calendar","description":"Local event calendar tool (now/list/add)","category":"Productivity","status":"Active"}
                        ]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/channels
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/channels") => {
                        let body = serde_json::json!([]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/sessions
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/sessions") => {
                        let body = serde_json::json!([]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/cli-tools
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/cli-tools") => {
                        let body = serde_json::json!([
                            {"name":"mexius","path":"/home/user/mexius/target/release/mexius","version":null,"category":"system"}
                        ]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/config — read/write config.toml
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/config") => {
                        let config_str = format!("{}/config.toml", &*herma_root);
                        let content = tokio::fs::read_to_string(std::path::Path::new(&config_str)).await
                            .unwrap_or_else(|_| "# Mexius configuration\n".to_string());
                        let body = serde_json::json!({"content": content, "format": "toml"});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::PUT, "/api/config") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let config_str = format!("{}/config.toml", &*herma_root);
                        match tokio::fs::write(std::path::Path::new(&config_str), &whole).await {
                            Ok(()) => {
                                let mut res = Response::new(Body::empty());
                                *res.status_mut() = StatusCode::NO_CONTENT;
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(e) => {
                                let mut res = Response::new(Body::from(format!("Failed to write config: {}", e)));
                                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                        }
                    }

                    // ------------------------------------------------------------------
                    // /api/doctor — diagnostic check (same logic as herma doctor)
                    // ------------------------------------------------------------------
                    (&Method::POST, "/api/doctor") => {
                        let ollama_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
                        let body = serde_json::json!([
                            {
                                "severity": if ollama_ok { "ok" } else { "warn" },
                                "category": "ollama",
                                "message": if ollama_ok { "Ollama is reachable on port 11434" } else { "Ollama not reachable on port 11434" }
                            },
                            {"severity":"ok","category":"gateway","message":"Gateway is running"},
                            {"severity":"ok","category":"memory","message":"Memory backend initialized"}
                        ]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/soul (and legacy /api/v1/memory/soul) — Read / write soul.md
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/memory/soul") | (&Method::GET, "/api/soul") => {
                        let soul_path = format!("{}/soul.md", &*herma_root);
                        let content = tokio::fs::read_to_string(&soul_path).await
                            .unwrap_or_else(|_| "# Soul\n\nDefine your agent's identity here.\n".to_string());
                        let body = serde_json::json!({"content": content});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::POST, "/api/memory/soul")
                    | (&Method::PUT, "/api/memory/soul")
                    | (&Method::POST, "/api/soul")
                    | (&Method::PUT, "/api/soul") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        // Accept either raw text or {"content":"..."} JSON
                        let markdown: String = serde_json::from_slice::<serde_json::Value>(&whole)
                            .ok()
                            .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()))
                            .unwrap_or_else(|| String::from_utf8_lossy(&whole).into_owned());
                        let soul_path = format!("{}/soul.md", &*herma_root);
                        match tokio::fs::write(&soul_path, markdown.as_bytes()).await {
                            Ok(()) => {
                                let mut res = Response::new(Body::from("{\"status\":\"saved\",\"reinitialized\":true}"));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(e) => {
                                let mut res = Response::new(Body::from(format!("{{\"error\":\"{}\"}}", e)));
                                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                        }
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/hardware  — CPU / RAM / GPU telemetry from /proc
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/hardware") => {
                        // CPU: two quick /proc/stat reads 200 ms apart → usage %
                        async fn read_stat_idle() -> Option<(u64, u64)> {
                            let s = tokio::fs::read_to_string("/proc/stat").await.ok()?;
                            let line = s.lines().next()?;
                            let nums: Vec<u64> = line.split_whitespace().skip(1)
                                .filter_map(|v| v.parse().ok()).collect();
                            if nums.len() < 4 { return None; }
                            let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
                            let total: u64 = nums.iter().sum();
                            Some((idle, total))
                        }

                        let snap1 = read_stat_idle().await;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        let snap2 = read_stat_idle().await;

                        let cpu_percent = match (snap1, snap2) {
                            (Some((i1, t1)), Some((i2, t2))) if t2 > t1 => {
                                let d_idle = i2.saturating_sub(i1) as f64;
                                let d_total = (t2 - t1) as f64;
                                ((1.0 - d_idle / d_total) * 100.0).clamp(0.0, 100.0)
                            }
                            _ => 0.0,
                        };

                        // RAM from /proc/meminfo
                        let (ram_total_gb, ram_used_gb, ram_percent) = match tokio::fs::read_to_string("/proc/meminfo").await {
                            Ok(s) => {
                                let mut total_kb = 0u64;
                                let mut avail_kb = 0u64;
                                for line in s.lines() {
                                    if line.starts_with("MemTotal:") {
                                        total_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                                    } else if line.starts_with("MemAvailable:") {
                                        avail_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                                    }
                                }
                                let total = total_kb as f64 / 1_048_576.0;
                                let used = (total_kb.saturating_sub(avail_kb)) as f64 / 1_048_576.0;
                                let pct = if total > 0.0 { (used / total * 100.0).clamp(0.0, 100.0) } else { 0.0 };
                                (total, used, pct)
                            }
                            Err(_) => (0.0, 0.0, 0.0),
                        };

                        // GPU / VRAM — probe Ollama ps endpoint
                        let client = reqwest::Client::builder()
                            .timeout(std::time::Duration::from_secs(2))
                            .build()
                            .unwrap_or_default();
                        let (gpu_percent, vram_used_gb, vram_total_gb, model_loaded, loaded_model) =
                            match client.get("http://127.0.0.1:11434/api/ps").send().await {
                                Ok(r) => {
                                    if let Ok(j) = r.json::<serde_json::Value>().await {
                                        let models = j.get("models").and_then(|m| m.as_array());
                                        let (mut vu, mut vt, mut name, mut active_model) = (0.0f64, 0.0f64, false, String::new());
                                        if let Some(list) = models {
                                            for m in list {
                                                let size = m.get("size_vram").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                                let total = m.get("size").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                                vu += size / 1_073_741_824.0;
                                                vt += total / 1_073_741_824.0;
                                                if let Some(model_name) = m.get("name").and_then(|v| v.as_str()) {
                                                    name = true;
                                                    if active_model.is_empty() {
                                                        active_model = model_name.to_string();
                                                    }
                                                }
                                            }
                                        }
                                        let gpu_pct = if vt > 0.0 { (vu / vt * 100.0).clamp(0.0, 100.0) } else { 0.0 };
                                        (gpu_pct, vu, vt, name, active_model)
                                    } else {
                                        (0.0, 0.0, 0.0, false, String::new())
                                    }
                                }
                                Err(_) => (0.0, 0.0, 0.0, false, String::new()),
                            };

                        let body = serde_json::json!({
                            "cpu_percent": (cpu_percent * 10.0).round() / 10.0,
                            "ram_used_gb": (ram_used_gb * 100.0).round() / 100.0,
                            "ram_total_gb": (ram_total_gb * 100.0).round() / 100.0,
                            "ram_percent": (ram_percent * 10.0).round() / 10.0,
                            "gpu_percent": (gpu_percent * 10.0).round() / 10.0,
                            "vram_used_gb": (vram_used_gb * 100.0).round() / 100.0,
                            "vram_total_gb": (vram_total_gb * 100.0).round() / 100.0,
                            "model_loaded": model_loaded,
                            "loaded_model": loaded_model
                        });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/ollama/pull — start pulling a local Ollama model in background
                    // ------------------------------------------------------------------
                    (&Method::POST, "/api/ollama/pull") => {
                        let whole = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let pull_req = serde_json::from_slice::<OllamaPullRequest>(&whole);

                        match pull_req {
                            Ok(body) => {
                                let model = body.model.trim().to_string();
                                if model.is_empty() {
                                    let mut bad = Response::new(Body::from("{\"error\":\"model is required\"}"));
                                    *bad.status_mut() = StatusCode::BAD_REQUEST;
                                    bad.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                    add_cors_headers(&mut bad);
                                    return Ok(bad);
                                }

                                let model_for_task = model.clone();
                                let log_sender = log_tx_req.clone();
                                tokio::spawn(async move {
                                    let _ = log_sender.send(format!("Starting Ollama pull for {}", model_for_task));
                                    match tokio::process::Command::new("ollama")
                                        .arg("pull")
                                        .arg(&model_for_task)
                                        .output()
                                        .await
                                    {
                                        Ok(output) => {
                                            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                                            let msg = if output.status.success() {
                                                format!("Ollama pull finished for {}{}{}", model_for_task, if stdout.is_empty() { "" } else { ": " }, stdout)
                                            } else {
                                                format!("Ollama pull failed for {}{}{}", model_for_task, if stderr.is_empty() { "" } else { ": " }, stderr)
                                            };
                                            let _ = log_sender.send(msg);
                                        }
                                        Err(err) => {
                                            let _ = log_sender.send(format!("Failed to start Ollama pull for {}: {}", model_for_task, err));
                                        }
                                    }
                                });

                                let body = serde_json::json!({
                                    "status": "started",
                                    "model": model,
                                    "message": format!("Started Ollama pull for {}. Refresh models in a few seconds.", model),
                                });
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(_) => {
                                let mut bad = Response::new(Body::from("{\"error\":\"invalid json\"}"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                bad.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut bad);
                                Ok(bad)
                            }
                        }
                    }

                    // ------------------------------------------------------------------
                    // /api/tools/status  — Mexius embedded tool availability probe
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/tools/status") => {
                        async fn probe_cmd(cmd: &str, args: &[&str]) -> bool {
                            tokio::process::Command::new(cmd).args(args)
                                .output().await.map(|o| o.status.success()).unwrap_or(false)
                        }
                        let (shell_ok, git_ok, find_ok, _curl_ok) = tokio::join!(
                            probe_cmd("sh", &["-lc", "printf ok >/dev/null"]),
                            probe_cmd("git", &["--version"]),
                            probe_cmd("find", &[".", "-maxdepth", "0"]),
                            probe_cmd("curl", &["--version"])
                        );
                        let camofox_ok = {
                            let camofox_url = std::env::var("CAMOFOX_URL").unwrap_or_default();
                            !camofox_url.trim().is_empty()
                        };
                        let body = serde_json::json!([
                            {
                                "name": "Shell",
                                "tool_id": "shell",
                                "description": "Execute shell commands via /bin/sh (raw, direct execution)",
                                "available": shell_ok,
                                "locked": false,
                                "icon": "Terminal"
                            },
                            {
                                "name": "Read File",
                                "tool_id": "read_file",
                                "description": "Read a file from disk and return its contents",
                                "available": true,
                                "locked": false,
                                "icon": "FileText"
                            },
                            {
                                "name": "Write File",
                                "tool_id": "write_file",
                                "description": "Write or overwrite a file on disk",
                                "available": true,
                                "locked": false,
                                "icon": "FilePen"
                            },
                            {
                                "name": "Append File",
                                "tool_id": "append_file",
                                "description": "Append content to an existing file",
                                "available": true,
                                "locked": false,
                                "icon": "FileStack"
                            },
                            {
                                "name": "List Directory",
                                "tool_id": "list_dir",
                                "description": "List contents of a directory",
                                "available": true,
                                "locked": false,
                                "icon": "FolderOpen"
                            },
                            {
                                "name": "Glob Search",
                                "tool_id": "glob_search",
                                "description": "Find files by name pattern using glob",
                                "available": find_ok,
                                "locked": false,
                                "icon": "Search"
                            },
                            {
                                "name": "Git",
                                "tool_id": "git",
                                "description": "Version control operations (status, log, diff, commit, etc.)",
                                "available": git_ok,
                                "locked": false,
                                "icon": "GitBranch"
                            },
                            {
                                "name": "HTTP Fetch",
                                "tool_id": "http_get",
                                "description": "Outbound HTTP/HTTPS GET requests via reqwest",
                                "available": true,
                                "locked": false,
                                "icon": "Globe"
                            },
                            {
                                "name": "Web Search",
                                "tool_id": "web_search",
                                "description": "Internet search (DuckDuckGo by default, optional Camofox provider)",
                                "available": true,
                                "locked": false,
                                "icon": "Globe"
                            },
                            {
                                "name": "Calendar",
                                "tool_id": "calendar",
                                "description": "Local calendar events (now/list/add)",
                                "available": true,
                                "locked": false,
                                "icon": "Calendar"
                            },
                            {
                                "name": "Camofox Browser",
                                "tool_id": "camofox",
                                "description": "Local anti-detection browser backend (requires CAMOFOX_URL)",
                                "available": camofox_ok,
                                "locked": false,
                                "icon": "Shield"
                            },
                            {
                                "name": "Memory",
                                "tool_id": "memory",
                                "description": "Persistent vector memory via LanceDB / Sqlite",
                                "available": true,
                                "locked": false,
                                "icon": "Brain"
                            },
                            {
                                "name": "Diagnostic",
                                "tool_id": "diagnostic",
                                "description": "System health probe — Ollama, PIN, port, toolset",
                                "available": true,
                                "locked": false,
                                "icon": "Stethoscope"
                            }
                        ]);
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/logs  — snapshot of recent gateway.log lines
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/logs") => {
                        let log_path = format!("{}/run_logs/gateway.log", &*herma_root);

                        let limit = req
                            .uri()
                            .query()
                            .and_then(|q| {
                                q.split('&')
                                    .find(|p| p.starts_with("limit="))
                                    .and_then(|p| p.split('=').nth(1))
                            })
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(200)
                            .min(2000);

                        let body = match tokio::fs::read_to_string(&log_path).await {
                            Ok(content) => {
                                let lines: Vec<String> = content
                                    .lines()
                                    .rev()
                                    .take(limit)
                                    .map(|s| s.to_string())
                                    .collect::<Vec<String>>()
                                    .into_iter()
                                    .rev()
                                    .collect();
                                serde_json::json!({"lines": lines, "count": lines.len()})
                            }
                            Err(_) => serde_json::json!({"lines": [], "count": 0}),
                        };

                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /api/v1/logs/stream  — SSE tail of run_logs/gateway.log
                    // ------------------------------------------------------------------
                    (&Method::GET, "/api/logs/stream") => {
                        use tokio::io::AsyncSeekExt;

                        let log_path = format!("{}/run_logs/gateway.log", &*herma_root);
                        let (tx_sse, rx_sse) = tokio::sync::mpsc::channel::<String>(256);

                        tokio::spawn(async move {
                            let mut file = match tokio::fs::OpenOptions::new().read(true).open(&log_path).await {
                                Ok(f) => f,
                                Err(_) => {
                                    let _ = tx_sse.send("data: {\"line\":\"[log file not found]\"}\n\n".to_string()).await;
                                    return;
                                }
                            };
                            // Seek near end (last ~8 KB) for initial tail
                            let meta = file.metadata().await.ok();
                            if let Some(m) = meta {
                                let seek_pos = m.len().saturating_sub(8192);
                                let _ = file.seek(std::io::SeekFrom::Start(seek_pos)).await;
                            }
                            // Scan to next newline boundary
                            let mut scan_buf = [0u8; 1];
                            loop {
                                match file.read_exact(&mut scan_buf).await {
                                    Ok(_) if scan_buf[0] == b'\n' => break,
                                    Ok(_) => {}
                                    Err(_) => break,
                                }
                            }
                            // Tail loop
                            let mut line_buf = String::new();
                            let mut read_buf = [0u8; 512];
                            loop {
                                match file.read(&mut read_buf).await {
                                    Ok(0) => {
                                        // EOF — wait and poll
                                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    }
                                    Ok(n) => {
                                        let chunk = String::from_utf8_lossy(&read_buf[..n]);
                                        for ch in chunk.chars() {
                                            if ch == '\n' {
                                                let trimmed = line_buf.trim().to_string();
                                                if !trimmed.is_empty() {
                                                    let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
                                                    let payload = format!("data: {{\"line\":\"{}\"}}\n\n", escaped);
                                                    if tx_sse.send(payload).await.is_err() { return; }
                                                }
                                                line_buf.clear();
                                            } else {
                                                line_buf.push(ch);
                                            }
                                        }
                                    }
                                    Err(_) => return,
                                }
                            }
                        });

                        // Build a streaming Body from the receiver channel using
                        // hyper::Body::channel() — no extra crate needed.
                        let (mut body_tx, body) = hyper::Body::channel();
                        tokio::spawn(async move {
                            let mut rx = rx_sse;
                            while let Some(msg) = rx.recv().await {
                                if body_tx.send_data(Bytes::from(msg)).await.is_err() {
                                    break;
                                }
                            }
                        });
                        let mut res = Response::new(body);
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/event-stream; charset=utf-8"));
                        res.headers_mut().insert(hyper::header::CACHE_CONTROL, hyper::header::HeaderValue::from_static("no-cache"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /ws/nexus — WebSocket for internal Chain-of-Thought stream
                    // Same upgrade pattern as /ws/chat but emits only "thinking" frames.
                    // ------------------------------------------------------------------
                    (&Method::GET, "/ws/nexus") => {
                        use ring::digest::{Context as DigestCtx, SHA1_FOR_LEGACY_USE_ONLY};
                        use base64::Engine as _;

                        let ws_key = req.headers()
                            .get("Sec-WebSocket-Key")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string());
                        let is_upgrade = req.headers()
                            .get(hyper::header::UPGRADE)
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_lowercase()) == Some("websocket".to_string());

                        let key = match (ws_key, is_upgrade) {
                            (Some(k), true) => k,
                            _ => {
                                let mut bad = Response::new(Body::from("Expected WebSocket upgrade"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                add_cors_headers(&mut bad);
                                return Ok::<_, hyper::Error>(bad);
                            }
                        };

                        let mut sha_ctx = DigestCtx::new(&SHA1_FOR_LEGACY_USE_ONLY);
                        sha_ctx.update(key.as_bytes());
                        sha_ctx.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
                        let sha_bytes = sha_ctx.finish();
                        let accept = base64::engine::general_purpose::STANDARD.encode(sha_bytes.as_ref());

                        let upgrade = hyper::upgrade::on(req);
                        let log_tx_nexus = log_tx_req.clone();
                        let sovereignty_nexus = sovereignty_state_req.clone();
                        // Subscribe to Nexus delegation events for this connection
                        let mut nexus_events_rx = nexus_tx_req.subscribe();

                        tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};

                            async fn ws_send_text(
                                stream: &mut hyper::upgrade::Upgraded,
                                text: &str,
                            ) -> bool {
                                let payload = text.as_bytes();
                                let len = payload.len();
                                let mut header: Vec<u8> = Vec::with_capacity(10);
                                header.push(0x81);
                                if len < 126 { header.push(len as u8); }
                                else if len < 65536 { header.push(0x7E); header.push((len >> 8) as u8); header.push((len & 0xFF) as u8); }
                                else { header.push(0x7F); for i in (0..8).rev() { header.push(((len >> (i * 8)) & 0xFF) as u8); } }
                                if stream.write_all(&header).await.is_err() { return false; }
                                if stream.write_all(payload).await.is_err() { return false; }
                                stream.flush().await.is_ok()
                            }

                            let mut upgraded = match upgrade.await {
                                Ok(u) => u,
                                Err(e) => { log::error!("Nexus WS upgrade error: {:?}", e); return; }
                            };

                            let current_state = sovereignty_nexus.read().await.to_string();
                            let connected_msg = serde_json::json!({
                                "type": "nexus_connected",
                                "message": "Nexus Chain-of-Thought stream active. Awaiting reasoning frames.",
                                "stream": "internal",
                                "sovereignty_state": current_state,
                            }).to_string();
                            if !ws_send_text(&mut upgraded, &connected_msg).await { return; }

                            let mut tick = 0u32;
                            let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(5));
                            heartbeat_interval.tick().await; // skip first immediate tick

                            loop {
                                tokio::select! {
                                    // Nexus delegation event — broadcast to client
                                    event = nexus_events_rx.recv() => {
                                        match event {
                                            Ok(ev) => {
                                                let event_type = if ev.result.is_some() { "agent_result" } else { "agent_delegation" };
                                                let frame = serde_json::json!({
                                                    "type": event_type,
                                                    "from_agent": ev.from_agent,
                                                    "to_agent": ev.to_agent,
                                                    "task": ev.task,
                                                    "result": ev.result,
                                                    "timestamp": ev.timestamp,
                                                });
                                                if !ws_send_text(&mut upgraded, &frame.to_string()).await { break; }
                                            }
                                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                                // Dropped frames — skip and continue
                                            }
                                            Err(_) => break,
                                        }
                                    }

                                    // Periodic heartbeat (every 5s)
                                    _ = heartbeat_interval.tick() => {
                                        tick += 1;
                                        let sovereignty_str = sovereignty_nexus.read().await.to_string();
                                        let heartbeat = serde_json::json!({
                                            "type": "heartbeat",
                                            "tick": tick,
                                            "stream": "nexus",
                                            "sovereignty_state": sovereignty_str,
                                        }).to_string();
                                        if !ws_send_text(&mut upgraded, &heartbeat).await { break; }

                                        // Check for incoming close frame (non-blocking)
                                        let mut h2 = [0u8; 2];
                                        match tokio::time::timeout(
                                            std::time::Duration::from_millis(5),
                                            upgraded.read_exact(&mut h2),
                                        ).await {
                                            Ok(Ok(_)) => {
                                                let opcode = h2[0] & 0x0F;
                                                if opcode == 0x8 { break; }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            let _ = upgraded.write_all(&[0x88, 0x00]).await;
                            let _ = upgraded.flush().await;
                            let _ = log_tx_nexus.send("[Nexus] Client disconnected".to_string());
                        });

                        let mut res = Response::builder()
                            .status(StatusCode::SWITCHING_PROTOCOLS)
                            .header(hyper::header::UPGRADE, "websocket")
                            .header(hyper::header::CONNECTION, "Upgrade")
                            .header("Sec-WebSocket-Accept", accept)
                            .header("Sec-WebSocket-Protocol", "mexius.v1")
                            .body(Body::empty())
                            .unwrap();
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ------------------------------------------------------------------
                    // /ws/chat — WebSocket upgrade for Agent Chat page
                    // Uses ring (SHA-1) + base64 already in the lock file via rustls.
                    // Frames are handled manually with tokio AsyncRead/AsyncWrite.
                    // ------------------------------------------------------------------
                    (&Method::GET, "/ws/chat") => {
                        let ws_key = req.headers()
                            .get("Sec-WebSocket-Key")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string());
                        let is_upgrade = req.headers()
                            .get(hyper::header::UPGRADE)
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_lowercase()) == Some("websocket".to_string());

                        let key = match (ws_key, is_upgrade) {
                            (Some(k), true) => k,
                            _ => {
                                let mut bad = Response::new(Body::from("Expected WebSocket upgrade"));
                                *bad.status_mut() = StatusCode::BAD_REQUEST;
                                add_cors_headers(&mut bad);
                                return Ok::<_, hyper::Error>(bad);
                            }
                        };

                        // Compute Sec-WebSocket-Accept = base64(sha1(key + WS_MAGIC))
                        use ring::digest::{Context as DigestCtx, SHA1_FOR_LEGACY_USE_ONLY};
                        use base64::Engine as _;
                        let mut sha_ctx = DigestCtx::new(&SHA1_FOR_LEGACY_USE_ONLY);
                        sha_ctx.update(key.as_bytes());
                        sha_ctx.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
                        let sha_bytes = sha_ctx.finish();
                        let accept = base64::engine::general_purpose::STANDARD.encode(sha_bytes.as_ref());

                        let upgrade = hyper::upgrade::on(req);
                        let log_tx_ws = log_tx_req.clone();
                        let herma_root_ws = herma_root.clone();

                        tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};

                            // Send an unmasked text frame to the client.
                            async fn ws_send_text(
                                stream: &mut hyper::upgrade::Upgraded,
                                text: &str,
                            ) -> bool {
                                let payload = text.as_bytes();
                                let len = payload.len();
                                let mut header: Vec<u8> = Vec::with_capacity(10);
                                header.push(0x81); // FIN=1, opcode=1 (text)
                                if len < 126 {
                                    header.push(len as u8);
                                } else if len < 65536 {
                                    header.push(0x7E);
                                    header.push((len >> 8) as u8);
                                    header.push((len & 0xFF) as u8);
                                } else {
                                    header.push(0x7F);
                                    for i in (0..8).rev() {
                                        header.push(((len >> (i * 8)) & 0xFF) as u8);
                                    }
                                }
                                if stream.write_all(&header).await.is_err() { return false; }
                                if stream.write_all(payload).await.is_err() { return false; }
                                stream.flush().await.is_ok()
                            }

                            let mut upgraded = match upgrade.await {
                                Ok(u) => u,
                                Err(e) => { log::error!("WS upgrade error: {:?}", e); return; }
                            };

                            // Send "connected" event
                            let connected = serde_json::json!({
                                "type": "connected",
                                "session_id": "local",
                                "resumed": false,
                                "message_count": 0
                            }).to_string();
                            if !ws_send_text(&mut upgraded, &connected).await { return; }

                            let hr = (*herma_root_ws).clone();
                            let mut conversation: Vec<serde_json::Value> = Vec::new();
                            // Cap conversation context at 40 messages (20 exchanges) to prevent
                            // unbounded memory growth and Ollama context window overflow.
                            const MAX_CONVERSATION_MSGS: usize = 40;

                            // Read config once at connection time; re-read each message so
                            // model changes in Config tab take effect without reconnecting.
                            let ollama_client = reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(300))
                                .build()
                                .unwrap_or_else(|_| reqwest::Client::new());

                            fn get_toml_top(toml: &str, key: &str) -> Option<String> {
                                let mut in_section = false;
                                for raw in toml.lines() {
                                    let line = raw.trim();
                                    if line.is_empty() || line.starts_with('#') { continue; }
                                    if line.starts_with('[') { in_section = true; continue; }
                                    if in_section { continue; }
                                    if let Some((lhs, rhs)) = line.split_once('=') {
                                        if lhs.trim() == key {
                                            return Some(rhs.trim().trim_matches('"').to_string());
                                        }
                                    }
                                }
                                None
                            }

                            // Frame read loop
                            loop {
                                let mut h2 = [0u8; 2];
                                if upgraded.read_exact(&mut h2).await.is_err() { break; }

                                let opcode = h2[0] & 0x0F;
                                let masked = (h2[1] & 0x80) != 0;
                                let mut payload_len = (h2[1] & 0x7F) as usize;

                                if payload_len == 126 {
                                    let mut ext = [0u8; 2];
                                    if upgraded.read_exact(&mut ext).await.is_err() { break; }
                                    payload_len = u16::from_be_bytes(ext) as usize;
                                } else if payload_len == 127 {
                                    let mut ext = [0u8; 8];
                                    if upgraded.read_exact(&mut ext).await.is_err() { break; }
                                    payload_len = u64::from_be_bytes(ext) as usize;
                                }

                                let mask_key: Option<[u8; 4]> = if masked {
                                    let mut m = [0u8; 4];
                                    if upgraded.read_exact(&mut m).await.is_err() { break; }
                                    Some(m)
                                } else {
                                    None
                                };

                                // Clamp to 1 MiB to protect against oversized frames
                                if payload_len > 1_048_576 { break; }
                                let mut payload = vec![0u8; payload_len];
                                if upgraded.read_exact(&mut payload).await.is_err() { break; }
                                if let Some(mk) = mask_key {
                                    for (i, b) in payload.iter_mut().enumerate() {
                                        *b ^= mk[i % 4];
                                    }
                                }

                                match opcode {
                                    0x8 => break, // Close
                                    0x9 => {
                                        // Ping → Pong
                                        let plen = payload.len().min(125) as u8;
                                        let mut pong = vec![0x8A, plen];
                                        pong.extend_from_slice(&payload[..plen as usize]);
                                        let _ = upgraded.write_all(&pong).await;
                                        let _ = upgraded.flush().await;
                                    }
                                    0x1 | 0x2 => {
                                        // Text or binary frame — dispatch to Ollama
                                        let text = String::from_utf8_lossy(&payload).into_owned();
                                        let _ = log_tx_ws.send(format!("[WS:chat] recv {} bytes", text.len()));
                                        let msg_val: serde_json::Value = match serde_json::from_str(&text) {
                                            Ok(v) => v,
                                            Err(_) => {
                                                let e = serde_json::json!({"type":"error","code":"INVALID_JSON","message":"Invalid JSON"}).to_string();
                                                if !ws_send_text(&mut upgraded, &e).await { break; }
                                                continue;
                                            }
                                        };
                                        let msg_type = msg_val.get("type").and_then(|v| v.as_str()).unwrap_or("message");
                                        let content = msg_val.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        if msg_type == "message" && !content.is_empty() {
                                            conversation.push(serde_json::json!({"role":"user","content":content}));
                                            // Trim oldest turns if over cap (keep system balance: drop oldest user+assistant pair)
                                            if conversation.len() > MAX_CONVERSATION_MSGS {
                                                let excess = conversation.len() - MAX_CONVERSATION_MSGS;
                                                conversation.drain(0..excess);
                                            }
                                            let cfg = tokio::fs::read_to_string(format!("{}/config.toml", hr)).await.unwrap_or_default();
                                            let provider = get_toml_top(&cfg, "default_provider").unwrap_or_default();
                                            let model = get_toml_top(&cfg, "default_model").unwrap_or_default();
                                            if provider == "ollama" && !model.is_empty() {
                                                let body = serde_json::json!({"model":model,"messages":conversation,"stream":false});
                                                match ollama_client.post("http://127.0.0.1:11434/api/chat").json(&body).send().await {
                                                    Ok(resp) => match resp.json::<serde_json::Value>().await {
                                                        Ok(j) => {
                                                            let reply = j.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("").to_string();
                                                            conversation.push(serde_json::json!({"role":"assistant","content":reply}));
                                                            let chunk = serde_json::json!({"type":"chunk","content":reply}).to_string();
                                                            if !ws_send_text(&mut upgraded, &chunk).await { break; }
                                                            let done = serde_json::json!({"type":"done","full_response":reply}).to_string();
                                                            if !ws_send_text(&mut upgraded, &done).await { break; }
                                                        }
                                                        Err(e) => {
                                                            let err = serde_json::json!({"type":"error","code":"PROVIDER_ERROR","message":format!("Ollama parse error: {}", e)}).to_string();
                                                            if !ws_send_text(&mut upgraded, &err).await { break; }
                                                        }
                                                    },
                                                    Err(e) => {
                                                        let err = serde_json::json!({"type":"error","code":"PROVIDER_ERROR","message":format!("Ollama error: {}", e)}).to_string();
                                                        if !ws_send_text(&mut upgraded, &err).await { break; }
                                                    }
                                                }
                                            } else {
                                                let err = serde_json::json!({"type":"error","code":"AGENT_INIT_FAILED","message":format!("Provider '{}' not configured. Set provider=ollama and a model in Config.", provider)}).to_string();
                                                if !ws_send_text(&mut upgraded, &err).await { break; }
                                            }
                                        }
                                    }
                                    _ => {} // ignore other opcodes (continuation, etc.)
                                }
                            }

                            // Send close frame
                            let _ = upgraded.write_all(&[0x88, 0x00]).await;
                            let _ = upgraded.flush().await;
                        });

                        let mut res = Response::builder()
                            .status(StatusCode::SWITCHING_PROTOCOLS)
                            .header(hyper::header::UPGRADE, "websocket")
                            .header(hyper::header::CONNECTION, "Upgrade")
                            .header("Sec-WebSocket-Accept", accept)
                            .header("Sec-WebSocket-Protocol", "mexius.v1")
                            .body(Body::empty())
                            .unwrap();
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::GET, "/api/memory") => {
                        // Backward-compatible endpoint used by the SPA: /api/memory
                        // Disk-first
                        let candidates = [
                            "/home/user/mexius/web/dist/api/memory.json",
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
                            "/home/user/mexius",
                        ];

                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = mexius_memory::SqliteMemory::new(p) {
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
                                "/home/user/mexius",
                            ];

                            let mut stored = false;
                            for ws in &ws_candidates {
                                let pstr = resolve_repo_path(ws);
                                let p = std::path::Path::new(&pstr);
                                if let Ok(mem) = mexius_memory::SqliteMemory::new(p) {
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
                            "/home/user/mexius",
                        ];
                        let mut removed = false;
                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = mexius_memory::SqliteMemory::new(p) {
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
                            "/home/user/mexius/web/dist/api/memories.json",
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

                        // Try live memory backend (Sqlite / LanceDB via mexius-memory crate)
                        // Fallbacks: try several plausible workspace dirs; first non-empty result wins.
                        let mut items_json: Option<Vec<serde_json::Value>> = None;
                        let ws_candidates = [
                            "/home/user/.openclaw",
                            "/home/user",
                            "/home/user/Project/zeroclaw",
                            "/home/user/mexius",
                        ];

                        for ws in &ws_candidates {
                            let pstr = resolve_repo_path(ws);
                            let p = std::path::Path::new(&pstr);
                            if let Ok(mem) = mexius_memory::SqliteMemory::new(p) {
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
                                    "/home/user/mexius",
                                ];
                                let mut removed = false;
                                for ws in &ws_candidates {
                                    let pstr = (resolve_repo_path)(ws);
                                    let p = std::path::Path::new(&pstr);
                                    if let Ok(mem) = mexius_memory::SqliteMemory::new(p) {
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
                            "/home/user/mexius/web/dist/api/admin/models/test.json",
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

                    (&Method::GET, "/api/lattice/top_k") => {
                        let json = crate::lattice::handle_top_k(&lattice_entity_req).await;
                        let body = json.to_string();
                        let mut res = Response::new(Body::from(body));
                        res.headers_mut().insert(
                            hyper::header::CONTENT_TYPE,
                            hyper::header::HeaderValue::from_static("application/json"),
                        );
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::POST, "/api/lattice/init") => {
                        let msg = crate::lattice::handle_init(&lattice_entity_req).await;
                        let mut res = Response::new(Body::from(msg));
                        res.headers_mut().insert(
                            hyper::header::CONTENT_TYPE,
                            hyper::header::HeaderValue::from_static("text/plain; charset=utf-8"),
                        );
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::POST, "/api/lattice/inject_word") => {
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let word = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                            .ok()
                            .and_then(|v| v.get("word").and_then(|w| w.as_str()).map(|s| s.to_owned()))
                            .unwrap_or_default();
                        crate::lattice::handle_inject(&lattice_entity_req, word).await;
                        let mut res = Response::new(Body::from("ok"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::OPTIONS, p) if p.starts_with("/api/lattice/") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // GET /api/state/status — current sovereignty state + meta
                    // ──────────────────────────────────────────────────────────
                    (&Method::GET, "/api/state/status") => {
                        let s = sovereignty_state_req.read().await;
                        let state_str = s.to_string();
                        drop(s);
                        let is_dreaming = state_str == "dreaming";
                        let body = serde_json::json!({
                            "state": state_str,
                            "db_read_only": is_dreaming,
                            "user_input_enabled": !is_dreaming,
                        });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // POST /api/state/toggle — switch sovereignty mode
                    // Body: { "state": "active" | "dreaming" | "nexus" | "idle" }
                    // ──────────────────────────────────────────────────────────
                    (&Method::POST, "/api/state/toggle") => {
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let requested: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
                        let state_str = requested["state"].as_str().unwrap_or("active");
                        let new_state = match state_str {
                            "idle"     => crate::sovereignty::SovereigntyState::Idle,
                            "dreaming" => crate::sovereignty::SovereigntyState::Dreaming,
                            "nexus"    => crate::sovereignty::SovereigntyState::Nexus,
                            _          => crate::sovereignty::SovereigntyState::Active,
                        };
                        {
                            let mut s = sovereignty_state_req.write().await;
                            *s = new_state;
                        }
                        let _ = log_tx_req.send(format!("[State] Sovereignty mode changed to: {}", new_state));
                        let body = serde_json::json!({"ok": true, "state": new_state.to_string()});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::OPTIONS, "/api/state/status") | (&Method::OPTIONS, "/api/state/toggle") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // GET /api/models — list all registered custom models
                    // ──────────────────────────────────────────────────────────
                    (&Method::GET, "/api/models") => {
                        let reg = model_registry_req.read().await;
                        let body = serde_json::to_string(&*reg).unwrap_or_else(|_| "[]".to_string());
                        drop(reg);
                        let mut res = Response::new(Body::from(body));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // POST /api/models — register a new custom model
                    // ──────────────────────────────────────────────────────────
                    (&Method::POST, "/api/models") => {
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                            Ok(v) => {
                                let custom_name = v["custom_name"].as_str().unwrap_or("Unnamed").to_string();
                                let model_id = v["model_id"].as_str().unwrap_or("").to_string();
                                let api_endpoint = v["api_endpoint"].as_str().unwrap_or("http://127.0.0.1:11434").to_string();
                                let api_key = v["api_key"].as_str().map(|s| s.to_string());
                                let source = v["source"].as_str().unwrap_or("ollama").to_string();

                                let entry = crate::model_registry::RegisteredModel::new(
                                    &custom_name, &model_id, &api_endpoint, api_key, &source,
                                );
                                let entry_id = entry.id.clone();
                                {
                                    let mut reg = model_registry_req.write().await;
                                    reg.push(entry);
                                    let snapshot = reg.clone();
                                    drop(reg);
                                    let _ = crate::model_registry::save_registry(&snapshot).await;
                                }
                                let body = serde_json::json!({"ok": true, "id": entry_id});
                                let mut res = Response::new(Body::from(body.to_string()));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                            Err(e) => {
                                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                let mut res = Response::new(Body::from(body.to_string()));
                                *res.status_mut() = StatusCode::BAD_REQUEST;
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                Ok(res)
                            }
                        }
                    }

                    // ──────────────────────────────────────────────────────────
                    // DELETE /api/models/:id — remove a registered model
                    // ──────────────────────────────────────────────────────────
                    (&Method::DELETE, p) if p.starts_with("/api/models/") => {
                        let id = p.trim_start_matches("/api/models/").to_string();
                        let removed = {
                            let mut reg = model_registry_req.write().await;
                            let before = reg.len();
                            reg.retain(|m| m.id != id);
                            let changed = reg.len() < before;
                            if changed {
                                let snapshot = reg.clone();
                                drop(reg);
                                let _ = crate::model_registry::save_registry(&snapshot).await;
                            }
                            changed
                        };
                        let body = serde_json::json!({"ok": removed});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // PATCH /api/models/:id — update is_active or fields
                    // ──────────────────────────────────────────────────────────
                    (&Method::PATCH, p) if p.starts_with("/api/models/") => {
                        let id = p.trim_start_matches("/api/models/").to_string();
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let patch: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
                        let mut found = false;
                        {
                            let mut reg = model_registry_req.write().await;
                            for entry in reg.iter_mut() {
                                if entry.id == id {
                                    found = true;
                                    if let Some(v) = patch["custom_name"].as_str() { entry.custom_name = v.to_string(); }
                                    if let Some(v) = patch["model_id"].as_str() { entry.model_id = v.to_string(); }
                                    if let Some(v) = patch["api_endpoint"].as_str() { entry.api_endpoint = v.to_string(); }
                                    if let Some(v) = patch["source"].as_str() { entry.source = v.to_string(); }
                                    if let Some(v) = patch["is_active"].as_bool() { entry.is_active = v; }
                                    if let Some(v) = patch["api_key"].as_str() { entry.api_key = Some(v.to_string()); }
                                    break;
                                }
                            }
                            if found {
                                let snapshot = reg.clone();
                                drop(reg);
                                let _ = crate::model_registry::save_registry(&snapshot).await;
                            }
                        }
                        let body = serde_json::json!({"ok": found});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::OPTIONS, p) if p.starts_with("/api/models") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // ──────────────────────────────────────────────────────────
                    // POST /api/nexus/delegate — spawn a named Nexus sub-agent
                    // Body: { "agent": "Coder", "task": "...", "system_prompt": "..." }
                    // ──────────────────────────────────────────────────────────
                    (&Method::POST, "/api/nexus/delegate") => {
                        // Only allowed in Nexus mode
                        {
                            let s = sovereignty_state_req.read().await;
                            if *s != crate::sovereignty::SovereigntyState::Nexus {
                                drop(s);
                                let body = serde_json::json!({"ok": false, "error": "Not in Nexus mode"});
                                let mut res = Response::new(Body::from(body.to_string()));
                                *res.status_mut() = StatusCode::CONFLICT;
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                                add_cors_headers(&mut res);
                                return Ok::<_, hyper::Error>(res);
                            }
                        }
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
                        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
                        let agent_name = v["agent"].as_str().unwrap_or("Sub-Agent").to_string();
                        let task = v["task"].as_str().unwrap_or("").to_string();
                        let system_prompt = v["system_prompt"].as_str().unwrap_or("").to_string();

                        // Look up model override from registry
                        let model_override = {
                            let reg = model_registry_req.read().await;
                            crate::model_registry::find_by_name(&reg, &agent_name)
                                .map(|m| m.model_id.clone())
                        };

                        let _handle = crate::sovereignty::spawn_sub_agent(
                            agent_name.clone(),
                            system_prompt,
                            task.clone(),
                            (*nexus_tx_req).clone(),
                            model_override,
                        );

                        let body = serde_json::json!({"ok": true, "agent": agent_name, "task": task});
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json; charset=utf-8"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    (&Method::OPTIONS, "/api/nexus/delegate") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // GET /api/nexus/supervisor-prompt — return the Mexius Supervisor system prompt
                    (&Method::GET, "/api/nexus/supervisor-prompt") => {
                        let prompt = crate::sovereignty::get_supervisor_prompt();
                        let body = serde_json::json!({ "prompt": prompt });
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }
                    (&Method::OPTIONS, "/api/nexus/supervisor-prompt") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    // GET /api/ollama/ps — proxy to Ollama /api/ps for VRAM/loaded model info
                    (&Method::GET, "/api/ollama/ps") => {
                        let ollama_result = async {
                            let client = reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(5))
                                .build()?;
                            let resp = client.get("http://127.0.0.1:11434/api/ps").send().await?;
                            let json: serde_json::Value = resp.json().await?;
                            Ok::<_, reqwest::Error>(json)
                        }.await;
                        let body = match ollama_result {
                            Ok(json) => json,
                            Err(_) => serde_json::json!({ "models": [] }),
                        };
                        let mut res = Response::new(Body::from(body.to_string()));
                        res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json"));
                        add_cors_headers(&mut res);
                        Ok(res)
                    }
                    (&Method::OPTIONS, "/api/ollama/ps") => {
                        let mut res = Response::new(Body::empty());
                        add_cors_headers(&mut res);
                        Ok(res)
                    }

                    _ => {
                        // If this is a GET request, try to serve static files from the built web output.
                        if req.method() == &Method::GET {
                            if let Some((bytes, ct)) = try_serve_static_file(&*herma_root, req.uri().path()).await {
                                let mut res = Response::new(Body::from(bytes));
                                res.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static(ct));
                                add_cors_headers(&mut res);
                                return Ok::<_, hyper::Error>(res);
                            }
                        }

                        let mut not_found = Response::default();
                        *not_found.status_mut() = StatusCode::NOT_FOUND;
                        add_cors_headers(&mut not_found);
                        Ok(not_found)
                    }
                }
            }
        });

        // Spawn a task to serve this connection (with_upgrades enables WebSocket)
        tokio::spawn(async move {
            if let Err(err) = Http::new().serve_connection(stream, svc).with_upgrades().await {
                log::error!("Gateway connection error: {:?}", err);
            }
        });
    }
}
