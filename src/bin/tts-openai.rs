use async_openai::{
    config::OpenAIConfig,
    types::{CreateSpeechRequestArgs, SpeechModel, SpeechResponseFormat, Voice},
    Audio, Client,
};
use clap::Parser;
use std::io::Cursor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    text: String,

    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    let client = Client::new();
    let speech_model = SpeechModel::Tts1;
    let voice = Voice::Nova;
    let data = synthesize(&client, &args.text, speech_model, voice)
        .await
        .unwrap()
        .unwrap();
    std::fs::write(&args.output, data)?;

    Ok(())
}

async fn synthesize(
    client: &Client<OpenAIConfig>,
    text: &str,
    speech_model: SpeechModel,
    voice: Voice,
) -> anyhow::Result<Option<Vec<u8>>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }

    log::trace!("Synthesizing {:?}", &text);
    let start_time = std::time::Instant::now();

    let request = CreateSpeechRequestArgs::default()
        .model(speech_model)
        .voice(voice)
        .input(text)
        .response_format(SpeechResponseFormat::Flac)
        .build()?;

    let audio = Audio::new(&client);
    let response = audio.speech(request).await?;

    let flac_data: Vec<u8> = response.bytes.to_vec();
    let wav_data = decode_flac(&flac_data)?;

    log::trace!(
        "synthesize_speech took {}ms, {} bytes input, {} bytes output",
        start_time.elapsed().as_millis(),
        text.len(),
        wav_data.len()
    );
    Ok(Some(wav_data))
}

fn decode_flac(flac_data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(&flac_data);
    let mut reader = claxon::FlacReader::new(cursor)?;

    let spec = hound::WavSpec {
        channels: reader.streaminfo().channels as u16,
        sample_rate: reader.streaminfo().sample_rate,
        bits_per_sample: reader.streaminfo().bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut wav_data = Vec::new();
    {
        let cursor = Cursor::new(&mut wav_data);
        let mut wav_writer = hound::WavWriter::new(cursor, spec)?;

        for opt_sample in reader.samples() {
            let sample = opt_sample?;
            wav_writer.write_sample(sample)?;
        }
    }

    Ok(wav_data)
}
