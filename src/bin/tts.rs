use clap::Parser;
use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::{
    synthesis_input::InputSource, AudioConfig, AudioEncoding, SsmlVoiceGender, SynthesisInput,
    SynthesizeSpeechRequest, VoiceSelectionParams,
};
use google_cognitive_apis::texttospeech::synthesizer::Synthesizer;
use std::env;
use std::fs;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    voice: String,

    #[arg(long)]
    text: String,

    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut synthesizer = Synthesizer::create(credentials()?).await.unwrap();
    let language_code = &args.voice[..5];
    let data = synthesize(&mut synthesizer, &args.text, language_code, &args.voice)
        .await
        .unwrap()
        .unwrap();
    std::fs::write(&args.output, data)?;

    Ok(())
}

async fn synthesize(
    synthesizer: &mut Synthesizer,
    text: &str,
    language_code: &str,
    voice: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }

    log::trace!("Synthesizing {:?}", &text);
    let start_time = std::time::Instant::now();
    let response = synthesizer
        .synthesize_speech(SynthesizeSpeechRequest {
            input: Some(SynthesisInput {
                input_source: Some(InputSource::Text(text.to_owned())),
            }),
            voice: Some(VoiceSelectionParams {
                language_code: language_code.to_owned(),
                name: voice.to_owned(),
                ssml_gender: SsmlVoiceGender::Female as i32,
            }),
            audio_config: Some(AudioConfig {
                audio_encoding: AudioEncoding::Linear16 as i32,
                speaking_rate: 1f64,
                pitch: 0f64,
                volume_gain_db: 0f64,
                sample_rate_hertz: 24000,
                effects_profile_id: vec![],
            }),
        })
        .await
        .map_err(|e| anyhow::anyhow!("synthesize_speech error: {:?}", e))?;

    let data: Vec<u8> = response.audio_content;
    log::trace!(
        "synthesize_speech took {}ms, {} bytes input, {} bytes output",
        start_time.elapsed().as_millis(),
        text.len(),
        data.len()
    );
    Ok(Some(data))
}

fn credentials() -> Result<String, anyhow::Error> {
    let path = env::var("GOOGLE_APPLICATION_CREDENTIALS")?;
    Ok(fs::read_to_string(path)?)
}
