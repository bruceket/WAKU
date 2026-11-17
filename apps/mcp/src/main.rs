//! `waku-mcp`: server MCP stdio che fa da ponte verso il desktop Waku.

use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = mcp::WakuMcp::from_env()?;
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
