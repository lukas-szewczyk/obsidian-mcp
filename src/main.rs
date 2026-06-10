use obsidian_mcp::ObsidianMcp;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let service = ObsidianMcp::from_env().map_err(|error| error.to_string())?;
    service
        .validate_vault()
        .await
        .map_err(|error| error.to_string())?;
    let running = service
        .serve(stdio())
        .await
        .map_err(|error| format!("MCP initialization failed: {error:?}"))?;

    running
        .waiting()
        .await
        .map_err(|error| format!("MCP server failed: {error:?}"))
        .map(|_| ())
}
