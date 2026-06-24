mod handler;
mod ui;
mod agent;
use std::io;
use handler::Handler;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut handler = Handler::new();
    handler.run().await?;
    Ok(())
}