use clap::Parser;
use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::SsmlVoiceGender;
use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::{
    synthesis_input::InputSource, AudioConfig, AudioEncoding, SynthesisInput,
    SynthesizeSpeechRequest, VoiceSelectionParams,
};
use google_cognitive_apis::texttospeech::synthesizer::Synthesizer;
use std::env;
use std::fs;
use std::io::Cursor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Prompt.
    #[arg(short, long, default_value = "Testing text to speech.")]
    text: String,

    #[arg(short, long, default_value = "en-US-Studio-O")]
    voice: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let credentials = fs::read_to_string(
        env::var("GOOGLE_APPLICATION_CREDENTIALS")
            .expect("missing GOOGLE_APPLICATION_CREDENTIALS environment variable"),
    )
    .unwrap();

    let mut synthesizer = Synthesizer::create(credentials).await.unwrap();

    let response = synthesizer
        .synthesize_speech(SynthesizeSpeechRequest {
            input: Some(SynthesisInput {
                input_source: Some(InputSource::Text(args.text)),
            }),
            voice: Some(VoiceSelectionParams {
                language_code: "en-us".to_string(),
                name: args.voice,
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
        .unwrap();

    let (_output_stream, output_stream_handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&output_stream_handle).unwrap();
    let data: Vec<u8> = response.audio_content;
    let cursor = Cursor::new(data);
    sink.append(rodio::Decoder::new(cursor)?);
    sink.sleep_until_end();

    Ok(())
}
