use anyhow::Result;
use clap::Parser;
use matrix_send::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    matrix_send::execute_main_logic(cli).await
}
