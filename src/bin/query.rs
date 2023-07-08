use async_openai::Client;
use clap::Parser;
use tokio::sync::mpsc;

use storyteller::query_story;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    prompt: String,
}

async fn print_tokens(mut token_rx: mpsc::Receiver<String>) -> anyhow::Result<()> {
    while let Some(token) = token_rx.recv().await {
        print!("{}", token);
    }
    println!();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = Client::new();

    let (token_tx, token_rx) = mpsc::channel(100);

    tokio::spawn(async move {
        if let Err(e) = print_tokens(token_rx).await {
            log::error!("print_tokens error: {}", e);
        }
    });

    println!("Prompt: {:?}", args.prompt);
    println!();

    query_story(client, args.prompt, token_tx).await?;

    Ok(())
}
