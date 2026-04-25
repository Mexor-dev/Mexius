/// Model Registry — Dynamic "Model Mesh" for Mexius.
///
/// Stores custom-named model entries in ~/.mexius/model_registry.json.
/// Each entry maps a friendly display name ("The Strategist") to a concrete
/// model ID + API endpoint, enabling the Nexus to route messages to different
/// LLM backends (local Ollama, OpenAI, Anthropic).
///
/// When a message is dispatched to a registered custom model, the gateway
/// prepends: "You are <Custom Name>. You are collaborating in the Mexius Nexus."

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── Model Entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredModel {
    /// Unique UUID for this registry entry
    pub id: String,
    /// Human-readable display name shown in the UI (e.g. "The Strategist")
    pub custom_name: String,
    /// Actual model identifier (e.g. "llama3.1:8b", "gpt-4o", "claude-3-haiku-20240307")
    pub model_id: String,
    /// Full API base URL — e.g. "http://127.0.0.1:11434" for Ollama,
    /// "https://api.openai.com" for OpenAI
    pub api_endpoint: String,
    /// Optional API key; stored as-is (plaintext) — handle with care.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Source label for the UI dropdown: "ollama" | "openai" | "anthropic" | "custom"
    #[serde(default = "default_source")]
    pub source: String,
    /// Whether this model is currently active/available for routing
    #[serde(default = "bool_true")]
    pub is_active: bool,
    /// ISO-8601 creation timestamp
    pub created_at: String,
}

fn default_source() -> String { "ollama".to_string() }
fn bool_true() -> bool { true }

impl RegisteredModel {
    pub fn new(
        custom_name: impl Into<String>,
        model_id: impl Into<String>,
        api_endpoint: impl Into<String>,
        api_key: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            custom_name: custom_name.into(),
            model_id: model_id.into(),
            api_endpoint: api_endpoint.into(),
            api_key,
            source: source.into(),
            is_active: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// ─── Shared Registry ─────────────────────────────────────────────────────────

pub type SharedModelRegistry = Arc<RwLock<Vec<RegisteredModel>>>;

pub fn new_registry() -> SharedModelRegistry {
    Arc::new(RwLock::new(Vec::new()))
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn registry_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    format!("{}/.mexius/model_registry.json", home)
}

pub async fn load_registry(_mexius_root: &str) -> Vec<RegisteredModel> {
    let path = registry_path();
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str::<Vec<RegisteredModel>>(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub async fn save_registry(models: &[RegisteredModel]) -> Result<(), String> {
    let path = registry_path();
    // Ensure the directory exists
    if let Some(parent) = std::path::Path::new(&path).parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(models).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, content).await.map_err(|e| e.to_string())
}

// ─── Lookup Helpers ───────────────────────────────────────────────────────────

/// Find an active model by its custom display name (case-insensitive).
pub fn find_by_name<'a>(models: &'a [RegisteredModel], name: &str) -> Option<&'a RegisteredModel> {
    models.iter().find(|m| m.is_active && m.custom_name.eq_ignore_ascii_case(name))
}

/// Build the Nexus identity prefix injected before every message sent to a
/// custom-named model.
pub fn nexus_identity_prefix(model: &RegisteredModel) -> String {
    format!(
        "You are {}. You are collaborating in the Mexius Nexus.",
        model.custom_name
    )
}

// ─── Routing: Send a message to a custom model ───────────────────────────────

#[derive(Debug)]
pub struct CustomModelResponse {
    pub content: String,
    pub model_used: String,
    pub source: String,
}

/// Dispatch a chat message to a registered custom model.
/// Injects the Nexus identity prefix into the system prompt.
pub async fn dispatch_to_custom_model(
    model: &RegisteredModel,
    messages: &[serde_json::Value],
) -> Result<CustomModelResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let identity = nexus_identity_prefix(model);

    // Prepend system message with identity
    let mut full_messages = vec![serde_json::json!({
        "role": "system",
        "content": identity
    })];
    full_messages.extend_from_slice(messages);

    let (url, request_body) = match model.source.as_str() {
        "openai" | "anthropic" => {
            // OpenAI-compatible format
            let url = format!("{}/v1/chat/completions", model.api_endpoint.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model.model_id,
                "messages": full_messages,
                "stream": false
            });
            (url, body)
        }
        _ => {
            // Ollama format
            let url = format!("{}/api/chat", model.api_endpoint.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model.model_id,
                "messages": full_messages,
                "stream": false
            });
            (url, body)
        }
    };

    let mut req = client.post(&url).json(&request_body);

    if let Some(key) = &model.api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // Extract content from either Ollama or OpenAI response format
    let content = json["message"]["content"]
        .as_str()
        .or_else(|| json["choices"][0]["message"]["content"].as_str())
        .unwrap_or("(no response)")
        .to_string();

    Ok(CustomModelResponse {
        content,
        model_used: model.model_id.clone(),
        source: model.source.clone(),
    })
}
