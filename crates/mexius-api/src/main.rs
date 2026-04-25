// Cleaned and consolidated main.rs for mexius-api
use clap::{Parser, Subcommand};
use std::net::SocketAddr;

mod hermes;
mod tools;
mod compat;
mod gateway;
mod lattice;
mod memory_store;
mod sovereignty;
mod model_registry;

#[derive(Parser)]
#[command(name = "mexius", version, about = "Mexius Gateway / AI Agent Daemon", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Mexius agent daemon (agent + Web UI)
    Start,
    /// Run the gateway Web UI only
    Gateway {
        /// Optional address (default 0.0.0.0:42617)
        addr: Option<String>,
    },
    /// Run system checks (config + tools)
    Doctor,
    /// Onboard/setup the local workspace (placeholder)
    Onboard,
}

fn detect_local_ip() -> String {
    match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(sock) => {
            if sock.connect("8.8.8.8:80").is_ok() {
                if let Ok(local) = sock.local_addr() {
                    return local.ip().to_string();
                }
            }
            "127.0.0.1".to_string()
        }
        Err(_) => "127.0.0.1".to_string(),
    }
}

#[tokio::main]
async fn main() {
    // Logging setup
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Start => {
            let ip = detect_local_ip();
            log::info!("Mexius WebUI available at: http://{}:42617/", ip);
            log::info!("Starting Mexius agent + gateway...");
            log::info!("Initializing Mexius Entity");

            // Initialization / health check
            match compat::initialize().await {
                Ok(msg) => log::info!("Initialization health check: {}", msg),
                Err(err) => log::warn!("Initialization health check failed: {}", err),
            }

            // Initialize toolset (Shell, File, Git)
            match compat::init_tools().await {
                Ok(tools) => {
                    let desc = tools.into_iter().map(|(n,ok)| format!("{}:{}", n, ok)).collect::<Vec<_>>().join(", ");
                    log::info!("Toolset: {}", desc);
                }
                Err(e) => log::warn!("Tool initialization failed: {}", e),
            }

            // Mexius channel
            let (tx, mut rx) = tokio::sync::mpsc::channel::<hermes::Message>(32);
            // Log broadcaster for SSE / UI
            let (log_tx, _) = tokio::sync::broadcast::channel::<String>(128);
            // Thought streamer for Mexius internal reasoning (SSE)
            let (thought_tx, _) = tokio::sync::broadcast::channel::<String>(256);
            hermes::start_listener(tx.clone(), thought_tx.clone());

            // Start gateway (pass Hermes sender and log broadcaster)
            let gw_addr: SocketAddr = "0.0.0.0:42617".parse().expect("invalid gateway addr");
            let tx_for_gw = tx.clone();
            let log_tx_for_gw = log_tx.clone();
            let thought_tx_for_gw = thought_tx.clone();
            let lattice_entity = lattice::make_entity();
            lattice::spawn_pulse(lattice_entity.clone());
            let lattice_entity_gw = lattice_entity.clone();
            let gateway_handle = tokio::spawn(async move {
                if let Err(e) = gateway::run(gw_addr, tx_for_gw, log_tx_for_gw, thought_tx_for_gw, lattice_entity_gw).await {
                    log::error!("Gateway error: {:?}", e);
                }
            });

            // Dispatch loop: process Hermes messages and heartbeat
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        log::info!("Mexius agent heartbeat...");
                            let _ = log_tx.send("Mexius agent heartbeat...".to_string());
                    }
                    maybe_msg = rx.recv() => {
                        match maybe_msg {
                            Some(msg) => {
                                let received = format!("Received Mexius message: id={} intent={} content={}", msg.id, msg.intent, msg.content);
                                log::info!("{}", received);
                                let _ = log_tx.send(received);
                                let result = tools::run_tool(&msg).await;
                                match result {
                                    Ok(res) => {
                                        let rlog = format!("Mexius tool result for msg {}: {}", msg.id, res);
                                        log::info!("{}", rlog);
                                        let _ = log_tx.send(rlog);
                                        if let Err(e) = hermes::reply(&msg, &res).await {
                                            let w = format!("Failed to reply via Mexius: {}", e);
                                            log::warn!("{}", w);
                                            let _ = log_tx.send(w);
                                        }
                                    }
                                    Err(e) => {
                                        let elog = format!("Mexius tool execution failed: {}", e);
                                        log::error!("{}", elog);
                                        let _ = log_tx.send(elog);
                                    }
                                }
                            }
                            None => {
                                log::warn!("Mexius sender closed, exiting dispatch loop");
                                break;
                            }
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("Received Ctrl-C, shutting down...");
                        break;
                    }
                }
            }

            // Stop gateway
            gateway_handle.abort();
            log::info!("Mexius daemon shutting down.");
        }
        Commands::Gateway { addr } => {
            let addr_str = addr.unwrap_or_else(|| "0.0.0.0:42617".to_string());
            let gw_addr: SocketAddr = addr_str.parse().expect("invalid gateway addr");
            // For Gateway-only mode, create placeholder channels to satisfy the
            // gateway API (no Hermes backend running).
            let (tx, _rx) = tokio::sync::mpsc::channel::<hermes::Message>(8);
            let (log_tx, _log_rx) = tokio::sync::broadcast::channel::<String>(128);
            let (thought_tx, _thought_rx) = tokio::sync::broadcast::channel::<String>(256);
            let lattice_entity = lattice::make_entity();
            lattice::spawn_pulse(lattice_entity.clone());
            if let Err(e) = gateway::run(gw_addr, tx, log_tx, thought_tx, lattice_entity).await {
                log::error!("Gateway error: {:?}", e);
            }
        }
        Commands::Doctor => {
            let mut all_ok = true;
            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());

            // ── 1. Directory structure: ~/mexius must exist ───────────────────
            let mexius_dir = format!("{}/mexius", home);
            let dir_ok = tokio::fs::metadata(&mexius_dir).await.map(|m| m.is_dir()).unwrap_or(false);
            if dir_ok {
                log::info!("✅ Directory ~/mexius: OK ({})", mexius_dir);
            } else {
                log::warn!("❌ Directory ~/mexius: NOT FOUND (expected {})", mexius_dir);
                all_ok = false;
            }

            // ── 2. PIN hash: ~/.mexius/pin.json must exist ────────────────────
            let pin_path = format!("{}/.mexius/pin.json", home);
            let pin_ok = tokio::fs::metadata(&pin_path).await.map(|m| m.is_file()).unwrap_or(false);
            if pin_ok {
                log::info!("✅ PIN hash: OK ({})", pin_path);
            } else {
                log::warn!("❌ PIN hash: NOT SET — run 'mexius start' and set your PIN via the WebUI ({})", pin_path);
                all_ok = false;
            }

            // ── 3. LanceDB / memory store: ~/.mexius/lancedb_store ────────────
            let lancedb_path = format!("{}/.mexius/lancedb_store", home);
            let ldb_ok = tokio::fs::metadata(&lancedb_path).await.map(|m| m.is_dir()).unwrap_or(false);
            if ldb_ok {
                log::info!("✅ LanceDB store: OK ({})", lancedb_path);
            } else {
                // Try to create it
                match tokio::fs::create_dir_all(&lancedb_path).await {
                    Ok(_) => log::info!("✅ LanceDB store: Created ({})", lancedb_path),
                    Err(e) => {
                        log::warn!("❌ LanceDB store: NOT accessible ({}) — {}", lancedb_path, e);
                        all_ok = false;
                    }
                }
            }

            // ── 4. Nexus bridge: Ollama must be reachable at 127.0.0.1:11434 ─
            let ollama_ok = tokio::net::TcpStream::connect(("127.0.0.1", 11434)).await.is_ok();
            if ollama_ok {
                // Also check it responds to /api/tags
                let ollama_api = reqwest::get("http://127.0.0.1:11434/api/tags").await.map(|r| r.status().is_success()).unwrap_or(false);
                if ollama_api {
                    log::info!("✅ Nexus bridge (Ollama): OK — /api/tags responsive");
                } else {
                    log::info!("✅ Nexus bridge (Ollama): TCP open, /api/tags not responding");
                }
            } else {
                log::warn!("❌ Nexus bridge (Ollama): NOT reachable at 127.0.0.1:11434 — start Ollama first");
                all_ok = false;
            }

            // ── 5. Toolset availability ───────────────────────────────────────
            match compat::init_tools().await {
                Ok(tools) => {
                    for (n, ok) in tools {
                        if ok {
                            log::info!("✅ Tool {}: available", n);
                        } else {
                            log::warn!("⚠️  Tool {}: not available", n);
                        }
                    }
                }
                Err(e) => log::error!("Tool check failed: {}", e),
            }

            // ── 6. Gateway port ───────────────────────────────────────────────
            let port_ok = tokio::net::TcpListener::bind("127.0.0.1:42617").await.is_ok();
            if port_ok {
                log::info!("✅ Gateway port 42617: available");
            } else {
                log::info!("ℹ️  Gateway port 42617: in use (gateway may already be running)");
            }

            // ── 7. Config file ────────────────────────────────────────────────
            let cfg_path = format!("{}/mexius/config.toml", home);
            match tokio::fs::metadata(&cfg_path).await {
                Ok(md) => log::info!("✅ config.toml: {} bytes ({})", md.len(), cfg_path),
                Err(_) => log::warn!("⚠️  config.toml not found at {}", cfg_path),
            }

            if all_ok {
                log::info!("🔱 Mexius doctor: ALL CHECKS PASSED — system is sovereign-ready");
            } else {
                log::warn!("🔱 Mexius doctor: SOME CHECKS FAILED — review warnings above");
                std::process::exit(1);
            }
        }
        Commands::Onboard => {
            // Basic onboarding placeholder: create workspace directory and sample config
            log::info!("Running onboarding (placeholder)");
            let _ = tokio::fs::create_dir_all("workspace").await;
            let cfg = "# mexius config\nname = \"mexius-local\"\n";
            if let Err(e) = tokio::fs::write("workspace/config.toml", cfg).await {
                log::warn!("Failed to write onboarding config: {}", e);
            } else {
                log::info!("Wrote workspace/config.toml (placeholder)");
            }
        }
    }
}
