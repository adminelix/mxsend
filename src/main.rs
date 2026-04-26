use anyhow::Result;
use clap::Parser;
use matrix_send::{MessageSender, SendOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let opts = SendOptions::parse();
    MessageSender::new(opts).send().await
}
