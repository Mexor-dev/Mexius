// Cleaned and consolidated main.rs for goldclaw-api
use clap::{Parser, Subcommand};
use std::net::SocketAddr;

mod hermes;
mod tools;
mod zeroclaw;
mod gateway;
mod memory_store;

#[derive(Parser)]
#[command(name = "goldclaw", version, about = "Goldclaw AI Agent Daemon", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Goldclaw agent daemon (agent + Web UI)
    Start,
    /// Run the gateway Web UI only
    Gateway {
        /// Optional address (default 127.0.0.1:42617)
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
            log::info!("Goldclaw WebUI available at: http://{}:42617/", ip);
            log::info!("Starting Goldclaw agent + gateway...");
            log::info!("Initializing Goldclaw Entity");

            // ZeroClaw initialization / health check
            match zeroclaw::initialize().await {
                Ok(msg) => log::info!("ZeroClaw health check: {}", msg),
                Err(err) => log::warn!("ZeroClaw health check failed: {}", err),
            }

            // Initialize toolset (Shell, File, Git)
            match zeroclaw::init_tools().await {
                Ok(tools) => {
                    let desc = tools.into_iter().map(|(n,ok)| format!("{}:{}", n, ok)).collect::<Vec<_>>().join(", ");
                    log::info!("Toolset: {}", desc);
                }
                Err(e) => log::warn!("Tool initialization failed: {}", e),
            }

            // Hermes channel
            let (tx, mut rx) = tokio::sync::mpsc::channel::<hermes::Message>(32);
            // Log broadcaster for SSE / UI
            let (log_tx, _) = tokio::sync::broadcast::channel::<String>(128);
            // Thought streamer for Hermes internal reasoning (SSE)
            let (thought_tx, _) = tokio::sync::broadcast::channel::<String>(256);
            hermes::start_listener(tx.clone(), thought_tx.clone());

            // Start gateway (pass Hermes sender and log broadcaster)
            let gw_addr: SocketAddr = "127.0.0.1:42617".parse().expect("invalid gateway addr");
            let tx_for_gw = tx.clone();
            let log_tx_for_gw = log_tx.clone();
            let thought_tx_for_gw = thought_tx.clone();
            let gateway_handle = tokio::spawn(async move {
                if let Err(e) = gateway::run(gw_addr, tx_for_gw, log_tx_for_gw, thought_tx_for_gw).await {
                    log::error!("Gateway error: {:?}", e);
                }
            });

            // Dispatch loop: process Hermes messages and heartbeat
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        log::info!("Goldclaw agent heartbeat...");
                        let _ = log_tx.send("Goldclaw agent heartbeat...".to_string());
                    }
                    maybe_msg = rx.recv() => {
                        match maybe_msg {
                            Some(msg) => {
                                let received = format!("Received Hermes message: id={} intent={} content={}", msg.id, msg.intent, msg.content);
                                log::info!("{}", received);
                                let _ = log_tx.send(received);
                                let result = tools::run_tool(&msg).await;
                                match result {
                                    Ok(res) => {
                                        let rlog = format!("OpenClaw result for msg {}: {}", msg.id, res);
                                        log::info!("{}", rlog);
                                        let _ = log_tx.send(rlog);
                                        if let Err(e) = hermes::reply(&msg, &res).await {
                                            let w = format!("Failed to reply via Hermes: {}", e);
                                            log::warn!("{}", w);
                                            let _ = log_tx.send(w);
                                        }
                                    }
                                    Err(e) => {
                                        let elog = format!("OpenClaw execution failed: {}", e);
                                        log::error!("{}", elog);
                                        let _ = log_tx.send(elog);
                                    }
                                }
                            }
                            None => {
                                log::warn!("Hermes sender closed, exiting dispatch loop");
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
            log::info!("Goldclaw daemon shutting down.");
        }
        Commands::Gateway { addr } => {
            let addr_str = addr.unwrap_or_else(|| "127.0.0.1:42617".to_string());
            let gw_addr: SocketAddr = addr_str.parse().expect("invalid gateway addr");
            // For Gateway-only mode, create placeholder channels to satisfy the
            // gateway API (no Hermes backend running).
            let (tx, _rx) = tokio::sync::mpsc::channel::<hermes::Message>(8);
            let (log_tx, _log_rx) = tokio::sync::broadcast::channel::<String>(128);
            let (thought_tx, _thought_rx) = tokio::sync::broadcast::channel::<String>(256);
            if let Err(e) = gateway::run(gw_addr, tx, log_tx, thought_tx).await {
                log::error!("Gateway error: {:?}", e);
            }
        }
        Commands::Doctor => {
            // Check Rust/Cargo availability
            let rust_ok = std::process::Command::new("cargo").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
            if rust_ok {
                log::info!("Cargo is available");
            } else {
                log::warn!("Cargo is NOT available");
            }

            // Check shell tool permission
            let shell_ok = tokio::process::Command::new("sh").arg("-c").arg("echo ok").output().await.map(|o| o.status.success()).unwrap_or(false);
            if shell_ok {
                log::info!("Shell execution: OK");
            } else {
                log::warn!("Shell execution: FAILED");
            }

            // Check if gateway port is available
            let port = 42617;
            let addr = format!("127.0.0.1:{}", port);
            let port_ok = tokio::net::TcpListener::bind(&addr).await.is_ok();
            if port_ok {
                log::info!("Gateway port {} is available", port);
            } else {
                log::warn!("Gateway port {} is already in use", port);
            }

            // Check write permissions for toolset (try writing to /tmp)
            let tmp_test = tokio::fs::write("/tmp/goldclaw_write_test", b"test").await.is_ok();
            if tmp_test {
                log::info!("Write permission to /tmp: OK");
                let _ = tokio::fs::remove_file("/tmp/goldclaw_write_test").await;
            } else {
                log::warn!("No write permission to /tmp");
            }

            // Existing config/tool checks
            match tokio::fs::metadata("config.toml").await {
                Ok(md) => log::info!("Found config.toml ({} bytes)", md.len()),
                Err(_) => log::warn!("config.toml not found in current directory"),
            }
            match zeroclaw::init_tools().await {
                Ok(tools) => {
                    for (n, ok) in tools {
                        log::info!("Tool {}: {}", n, ok);
                    }
                }
                Err(e) => log::error!("Tool check failed: {}", e),
            }
        }
        Commands::Onboard => {
            // Basic onboarding placeholder: create workspace directory and sample config
            log::info!("Running onboarding (placeholder)");
            let _ = tokio::fs::create_dir_all("workspace").await;
            let cfg = "# goldclaw config\nname = \"goldclaw-local\"\n";
            if let Err(e) = tokio::fs::write("workspace/config.toml", cfg).await {
                log::warn!("Failed to write onboarding config: {}", e);
            } else {
                log::info!("Wrote workspace/config.toml (placeholder)");
            }
        }
    }
}
