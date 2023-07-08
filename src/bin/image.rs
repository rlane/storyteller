use async_openai::{
    types::{CreateImageRequestArgs, ImageSize, ResponseFormat},
    Client,
};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    text: String,

    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    let client = Client::new();
    let tmp_dir = tempdir::TempDir::new("example")?;

    let request = CreateImageRequestArgs::default()
        .prompt(args.text)
        .n(1)
        .response_format(ResponseFormat::Url)
        .size(ImageSize::S1024x1024)
        .user("storyteller")
        .build()?;

    let response = client.images().create(request).await?;

    let paths = response.save(&tmp_dir).await?;
    assert_eq!(paths.len(), 1);

    let path = paths.first().unwrap();
    std::fs::copy(path, args.output)?;

    tmp_dir.close()?;

    Ok(())
}
