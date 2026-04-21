use hyper::{service::service_fn, Body, Request, Response, Method, StatusCode, server::conn::Http};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, broadcast};
use std::time::{SystemTime, UNIX_EPOCH};
use hyper::body::Bytes;
use serde::Deserialize;

const INDEX_HTML: &str = r#"<!doctype html>
<html lang=\"en\">
<head>
  <meta charset=\"utf-8\">
  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">
  <title>Goldclaw Dashboard</title>
  <link rel=\"stylesheet\" href=\"/styles.css\">
  <style>body{margin:0;padding:0}</style>
</head>
<body>
  <div id=\"app\" class=\"app\">
    <header class=\"hdr\"><h1>Goldclaw</h1><div class=\"status\" id=\"status\">disconnected</div></header>
    <main>
      <div id=\"log\" class=\"log\"></div>
      <div class=\"controls\">
        <input id=\"cmd\" placeholder=\"Enter command (shell)\" />
        <button id=\"run\">Run</button>
      </div>
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

const APP_JS: &str = r#"document.addEventListener('DOMContentLoaded',()=>{const log=document.getElementById('log');const status=document.getElementById('status');const es=new EventSource('/logs');es.onopen=()=>{status.textContent='connected'};es.onerror=()=>{status.textContent='error'};es.onmessage=(e)=>{const d=document.createElement('div');d.textContent=e.data;log.appendChild(d);log.scrollTop=log.scrollHeight};document.getElementById('run').addEventListener('click',async ()=>{const v=document.getElementById('cmd').value;try{const res=await fetch('/api/command',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({intent:'run_tool:shell',content:v})});const txt=await res.text();const d=document.createElement('div');d.textContent='> '+v;log.appendChild(d);const o=document.createElement('div');o.textContent='<= '+txt;log.appendChild(o);log.scrollTop=log.scrollHeight}catch(err){const e=document.createElement('div');e.textContent='ERROR: '+err;log.appendChild(e)}});document.getElementById('cmd').addEventListener('keydown',e=>{if(e.key==='Enter'){document.getElementById('run').click();e.preventDefault()}});});"#;

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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    log::info!("Gateway listening on http://{}", addr);

    loop {
        let (stream, _peer) = listener.accept().await?;

        // Per-connection clones
        let tx_conn = tx.clone();
        let log_tx_conn = log_tx.clone();

        let svc = service_fn(move |req: Request<Body>| {
            let tx_req = tx_conn.clone();
            let log_tx_req = log_tx_conn.clone();
            async move {
                match (req.method(), req.uri().path()) {
                    (&Method::GET, "/") => {
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
                            let _ = sender.send_data(Bytes::from("data: connected\n\n")).await;
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        let line = format!("data: {}\n\n", msg.replace('\n', "\\n"));
                                        if let Err(err) = sender.send_data(Bytes::from(line)).await {
                                            log::debug!("SSE send error: {:?}", err);
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        let line = format!("data: [lagged {} messages]\n\n", n);
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
                                let sender = c.sender.unwrap_or_else(|| "web-ui".to_string());

                                let message = crate::hermes::Message { id: id.clone(), sender, content, intent };

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
                    _ => {
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
