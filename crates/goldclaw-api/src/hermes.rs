use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    // pub sender: String, // No longer used
    pub content: String,
    pub intent: String,
}

/// Start a simple Hermes listener that sends messages into the provided channel.
/// This spawns a background Tokio task that emits a demo message and stays alive.
pub fn start_listener(tx: mpsc::Sender<Message>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let demo = Message {
            id: "demo-1".to_string(),
            content: "Run diagnostic tool".to_string(),
            intent: "run_tool:diagnostic".to_string(),
        };
        if let Err(e) = tx.send(demo).await {
            log::error!("Hermes stub failed to send demo message: {}", e);
            return;
        }

        // Keep a lightweight loop alive to simulate a persistent listener.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// Send a reply back via Hermes. This stub logs the reply and returns Ok.
pub async fn reply(_msg: &Message, reply: &str) -> Result<(), String> {
    log::info!("Hermes reply to {}: {}", _msg.id, reply);
    Ok(())
}
