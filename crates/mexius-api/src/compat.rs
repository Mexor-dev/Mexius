/// Minimal Mexius foundation health check (compat layer).
/// Uses async probes so it can be called from tokio contexts.
pub async fn initialize() -> Result<String, String> {
    // We no longer rely on an external `openclaw` binary. The tool
    // execution layer is embedded inside the mexius binary.
    Ok("internal-openclaw-embedded".to_string())
}

/// Initialize Mexius toolset (Shell, File, Git) and report availability.
pub async fn init_tools() -> Result<Vec<(String, bool)>, String> {
    let mut tools: Vec<(String, bool)> = Vec::new();

    // Shell (bash/sh)
    let shell_ok = tokio::process::Command::new("bash").arg("--version").output().await.is_ok()
        || tokio::process::Command::new("sh").arg("--version").output().await.is_ok();
    tools.push(("shell".to_string(), shell_ok));

    // Git
    let git_ok = tokio::process::Command::new("git").arg("--version").output().await.is_ok();
    tools.push(("git".to_string(), git_ok));

    // File (ability to create/remove a temporary file)
    let file_ok = match tokio::fs::File::create("/tmp/mexius_file_test.tmp").await {
        Ok(_) => {
            let _ = tokio::fs::remove_file("/tmp/mexius_file_test.tmp").await;
            true
        }
        Err(_) => false,
    };
    tools.push(("file".to_string(), file_ok));

    Ok(tools)
}
