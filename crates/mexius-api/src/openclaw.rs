use crate::hermes::Message;

/// Compatibility wrapper — forwards to the embedded `tools` runner.
pub async fn call_tool(msg: &Message) -> Result<String, String> {
    crate::tools::run_tool(msg).await
}
