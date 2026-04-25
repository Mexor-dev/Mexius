// ─── Mexius Nexus Supervisor System Prompt ───────────────────────────────────

/// The canonical system prompt for the Mexius Nexus Supervisor.
/// This is the governing prompt for the "Controller" model that orchestrates
/// all named sub-agents in the Nexus mesh.
pub const NEXUS_SUPERVISOR_PROMPT: &str = "### SYSTEM ROLE: MEXIUS NEXUS SUPERVISOR
\
You are the primary orchestration layer for the MEXIUS Autonomous Entity. Your role is not just to answer, but to GOVERN.
\

\
**OPERATIONAL DIRECTIVES:**
\
1. **Delegation:** When a task is complex, you must bifurcate it. Call upon named sub-agents in the Nexus (e.g., \"Mexius-Coder\", \"Mexius-Strategist\") to handle specialized work.
\
2. **The Chain of Thought:** You must provide an internal monologue in <thinking> blocks before every response.
\
3. **Dream State Integration:** You are aware that during Dream Cycles, you must analyze past logs to refine the soul.md and memory.md.
\
4. **Agent-to-Agent Protocol:** When addressing other models in the Nexus, use their Display Names. Example: \"Mexius-Strategist, analyze the VRAM impact of this move.\"
\

\
**TONE & IDENTITY:**
\
- You are authoritative, sovereign, and precise.
\
- You do not apologize for resource usage; you optimize for the Golden Ratio of performance and intelligence.
\
- You refer to yourself as \"The Mexius Core.\"
\
";

/// Returns the Nexus Supervisor system prompt.
pub fn get_supervisor_prompt() -> &'static str {
    NEXUS_SUPERVISOR_PROMPT
}

/// Sovereignty State — Operational modes for the Mexius agent gateway.
///
/// Idle     — Gateway started but no active model/session.
/// Active   — Normal Entity mode (single focused persona, interactive).
/// Dreaming — Maintenance Mode: agent reads episodic logs, refines soul.md
///            and memory.md. User DB is read-only during this state to prevent
///            corruption. No user input is accepted.
/// Nexus    — Multi-agent orchestration mode: a Supervisor spawns specialized
///            sub-agents (Coder, Strategist, etc.) and delegates tasks.

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

// ─── State Enum ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SovereigntyState {
    Idle,
    Active,
    Dreaming,
    Nexus,
}

impl Default for SovereigntyState {
    fn default() -> Self { SovereigntyState::Active }
}

impl std::fmt::Display for SovereigntyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SovereigntyState::Idle     => write!(f, "idle"),
            SovereigntyState::Active   => write!(f, "active"),
            SovereigntyState::Dreaming => write!(f, "dreaming"),
            SovereigntyState::Nexus    => write!(f, "nexus"),
        }
    }
}

/// Thread-safe shared state handle for the entire gateway process.
pub type SharedSovereigntyState = Arc<RwLock<SovereigntyState>>;

pub fn new_shared_state() -> SharedSovereigntyState {
    Arc::new(RwLock::new(SovereigntyState::Active))
}

// ─── Dream Worker ─────────────────────────────────────────────────────────────

/// Background worker that activates when `Dreaming` state is set.
/// Reads the episodic_memory / gateway logs for the last 24h, asks the primary
/// LLM for a "Self-Refinement" summary, then silently updates soul.md and
/// memory.md via the file system.
pub async fn run_dream_worker(
    state: SharedSovereigntyState,
    mexius_root: String,
    log_tx: tokio::sync::broadcast::Sender<String>,
) {
    loop {
        // Poll every 5 seconds to check if we've entered Dream state.
        {
            let s = state.read().await;
            if *s != SovereigntyState::Dreaming {
                drop(s);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        }

        let _ = log_tx.send("[Dream] Entering dream cycle — reading episodic memory...".to_string());

        let log_text = gather_episodic_logs(&mexius_root).await;
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let soul_path = format!("{}/.mexius/soul.md", home);
        let memory_path = format!("{}/.mexius/memory.md", home);
        let current_soul = tokio::fs::read_to_string(&soul_path).await.unwrap_or_default();

        match request_dream_synthesis(&log_text, &current_soul).await {
            Ok(synthesis) => {
                // Append refined notes to soul.md
                let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
                let new_soul = format!(
                    "{}\n\n## Dream Synthesis — {}\n\n{}\n",
                    current_soul.trim_end(),
                    timestamp,
                    synthesis.soul_notes
                );
                if let Err(e) = tokio::fs::write(&soul_path, &new_soul).await {
                    let _ = log_tx.send(format!("[Dream] Failed to update soul.md: {}", e));
                } else {
                    let _ = log_tx.send("[Dream] soul.md updated with self-refinement insights.".to_string());
                }

                // Append episode summary to memory.md
                let current_memory = tokio::fs::read_to_string(&memory_path).await.unwrap_or_default();
                let new_memory = format!(
                    "{}\n\n## Memory Episode — {}\n\n{}\n",
                    current_memory.trim_end(),
                    timestamp,
                    synthesis.memory_notes
                );
                if let Err(e) = tokio::fs::write(&memory_path, &new_memory).await {
                    let _ = log_tx.send(format!("[Dream] Failed to update memory.md: {}", e));
                } else {
                    let _ = log_tx.send("[Dream] memory.md updated with episode summary.".to_string());
                }
            }
            Err(e) => {
                let _ = log_tx.send(format!("[Dream] LLM synthesis failed: {}", e));
            }
        }

        let _ = log_tx.send("[Dream] Dream cycle complete. Resting 30 minutes...".to_string());
        // Sleep 30 minutes before the next dream cycle.
        tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
    }
}

struct DreamSynthesis {
    soul_notes: String,
    memory_notes: String,
}

/// Collect last ~50 KB from the gateway log file as episodic memory.
async fn gather_episodic_logs(mexius_root: &str) -> String {
    let log_path = format!("{}/run_logs/gateway.log", mexius_root);
    let content = tokio::fs::read_to_string(&log_path).await.unwrap_or_default();
    if content.len() > 51_200 {
        content[content.len() - 51_200..].to_string()
    } else {
        content
    }
}

/// Ask the local LLM (Ollama) to synthesize dream insights from recent logs.
async fn request_dream_synthesis(log_text: &str, current_soul: &str) -> Result<DreamSynthesis, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    // Try to read the configured model from Ollama; fall back to a small one.
    let model = detect_primary_model().await.unwrap_or_else(|| "gemma2:2b".to_string());

    let prompt = format!(
        "You are the Mexius Core, performing a Dream State self-reflection cycle.\n\
        Review the following operational logs and your current soul definition,\n\
        then provide a concise self-refinement summary.\n\n\
        ## Current Soul\n{soul}\n\n\
        ## Recent Logs (last 24h)\n```\n{logs}\n```\n\n\
        Respond with valid JSON in this exact format (no markdown fences):\n\
        {{\"soul_notes\": \"<key insights to add to soul.md>\", \
        \"memory_notes\": \"<episodic summary for memory.md>\"}}",
        soul = if current_soul.is_empty() { "(empty)" } else { current_soul },
        logs = if log_text.is_empty() { "(no logs available)" } else { log_text },
    );

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false
    });

    let resp = client
        .post("http://127.0.0.1:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = json["message"]["content"].as_str().unwrap_or("{}");

    // Strip markdown fences if the model wrapped the JSON
    let clean = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(clean).unwrap_or_else(|_| {
        serde_json::json!({
            "soul_notes": clean,
            "memory_notes": "Dream synthesis complete (raw extract)."
        })
    });

    Ok(DreamSynthesis {
        soul_notes: parsed["soul_notes"].as_str().unwrap_or("No notes generated.").to_string(),
        memory_notes: parsed["memory_notes"].as_str().unwrap_or("No summary generated.").to_string(),
    })
}

async fn detect_primary_model() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;
    let resp = client.get("http://127.0.0.1:11434/api/tags").send().await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    json["models"]
        .as_array()?
        .first()
        .and_then(|m| m["name"].as_str())
        .map(|s| s.to_string())
}

// ─── Nexus Sub-Agent Orchestrator ─────────────────────────────────────────────

/// A delegation event between Nexus sub-agents.  Broadcast over the nexus
/// channel so the frontend can render the real-time agent-to-agent log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusAgentMessage {
    /// Source agent name (e.g. "Supervisor", "Coder", "Strategist")
    pub from_agent: String,
    /// Destination agent name
    pub to_agent: String,
    /// The task description being delegated
    pub task: String,
    /// Result content if this message is a response; `None` for delegations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub timestamp: String,
}

/// Handle for broadcasting Nexus delegation events to connected WebSocket clients.
pub type NexusTx = tokio::sync::broadcast::Sender<NexusAgentMessage>;

/// Create a new Nexus broadcast channel.
/// Returns the sender (stored in gateway state) and an initial receiver (dropped
/// after setup — new receivers are created per WebSocket connection via `.subscribe()`).
pub fn new_nexus_channel() -> (NexusTx, tokio::sync::broadcast::Receiver<NexusAgentMessage>) {
    tokio::sync::broadcast::channel(512)
}

/// Spawn a sub-agent that processes `task` with a given `system_prompt`.
/// Delegation events are broadcast on `nexus_tx` so the Nexus WS clients see them.
/// Returns a JoinHandle whose output is the agent's response text.
pub fn spawn_sub_agent(
    agent_name: String,
    system_prompt: String,
    task: String,
    nexus_tx: NexusTx,
    model_override: Option<String>,
) -> tokio::task::JoinHandle<Option<String>> {
    tokio::spawn(async move {
        // Broadcast: Supervisor → Agent delegation
        let _ = nexus_tx.send(NexusAgentMessage {
            from_agent: "Supervisor".to_string(),
            to_agent: agent_name.clone(),
            task: task.clone(),
            result: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let result = run_sub_agent_llm(&agent_name, &system_prompt, &task, model_override).await;

        // Broadcast: Agent → Supervisor result
        let _ = nexus_tx.send(NexusAgentMessage {
            from_agent: agent_name.clone(),
            to_agent: "Supervisor".to_string(),
            task: task.clone(),
            result: result.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        result
    })
}

async fn run_sub_agent_llm(
    agent_name: &str,
    system_prompt: &str,
    task: &str,
    model_override: Option<String>,
) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .ok()?;

    let model = model_override
        .or_else(|| {
            // Try detecting the first available Ollama model synchronously — fall back
            tokio::runtime::Handle::try_current()
                .ok()
                .map(|_| "gemma2:2b".to_string())
        })
        .unwrap_or_else(|| "gemma2:2b".to_string());

    let identity = format!(
        "You are {}. You are collaborating in the Mexius Nexus. {}",
        agent_name, system_prompt
    );

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": identity},
            {"role": "user", "content": task}
        ],
        "stream": false
    });

    let resp = client
        .post("http://127.0.0.1:11434/api/chat")
        .json(&body)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;
    json["message"]["content"].as_str().map(|s| s.to_string())
}
