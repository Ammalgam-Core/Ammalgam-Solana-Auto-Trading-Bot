use ammalgram_assistant::common::logger::init_tracing;
use ammalgram_assistant::engine::copy_trader::run_copy_trader;
use anyhow::Result;
use dotenvy::dotenv;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    init_tracing()?;
    run_copy_trader().await
}
