use tokio::sync::{mpsc, broadcast};
use chrono::Utc;
use crate::memory_store;

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    // pub sender: String, // No longer used
    pub content: String,
    pub intent: String,
}

/// Start a simple Hermes listener that sends messages into the provided channel.
/// This spawns a background Tokio task that emits a demo message and stays alive.
pub fn start_listener(tx: mpsc::Sender<Message>, thought_tx: broadcast::Sender<String>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let demo = Message {
            id: "demo-1".to_string(),
            content: "Run diagnostic tool".to_string(),
            intent: "run_tool:diagnostic".to_string(),
        };
        if let Err(e) = tx.send(demo.clone()).await {
            log::error!("Hermes stub failed to send demo message: {}", e);
            return;
        }

        // Emit an initial thought event into the thought broadcaster and record a pinned fragment
        let thought = serde_json::json!({
            "type": "thought",
            "id": "thought-1",
            "text": "Analyzing system state",
            "time": Utc::now().to_rfc3339(),
        });
        let _ = thought_tx.send(thought.to_string());
        let frag = memory_store::MemoryFragment {
            id: "mfrag-1".to_string(),
            label: "System".to_string(),
            text_chunk: "System diagnostic info placeholder".to_string(),
            vector_id: None,
            distance: None,
            ts: Utc::now().to_rfc3339(),
        };
        let _ = memory_store::add_fragment(frag).await;

        // Keep a lightweight loop alive to simulate a persistent listener and emit periodic thought/action events
        let mut counter: u64 = 2;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let text = format!("Internal reasoning tick {}", counter);
            let t = serde_json::json!({
                "type": "thought",
                "id": format!("thought-{}", counter),
                "text": text,
                "time": Utc::now().to_rfc3339(),
            });
            let _ = thought_tx.send(t.to_string());

            // Also push a small pinned memory fragment occasionally
            if (counter % 3) == 0 {
                let frag = memory_store::MemoryFragment {
                    id: format!("mfrag-{}", counter),
                    label: "AgentContext".to_string(),
                    text_chunk: format!("Pinned context fragment at {}", Utc::now().to_rfc3339()),
                    vector_id: None,
                    distance: None,
                    ts: Utc::now().to_rfc3339(),
                };
                let _ = memory_store::add_fragment(frag).await;
            }

            counter += 1;
        }
    });
}

/// Send a reply back via Hermes. This stub logs the reply and returns Ok.
pub async fn reply(_msg: &Message, reply: &str) -> Result<(), String> {
    log::info!("Hermes reply to {}: {}", _msg.id, reply);
    Ok(())
}
