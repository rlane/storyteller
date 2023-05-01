use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::SsmlVoiceGender;
use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::{
    synthesis_input::InputSource, AudioConfig, AudioEncoding, SynthesisInput,
    SynthesizeSpeechRequest, VoiceSelectionParams,
};
use google_cognitive_apis::texttospeech::synthesizer::Synthesizer;
use std::env;
use std::fs;
use std::io::Cursor;

const VOICE: &str = "en-US-Studio-O";

#[allow(dead_code)]
pub struct Speaker {
    synthesizer: Synthesizer,
    output_stream: rodio::OutputStream,
    output_stream_handle: rodio::OutputStreamHandle,
    sink: rodio::Sink,
}

impl Speaker {
    pub async fn new() -> Self {
        let (output_stream, output_stream_handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&output_stream_handle).unwrap();

        let credentials = fs::read_to_string(
            env::var("GOOGLE_APPLICATION_CREDENTIALS")
                .expect("missing GOOGLE_APPLICATION_CREDENTIALS environment variable"),
        )
        .unwrap();
        let synthesizer = Synthesizer::create(credentials).await.unwrap();

        Self {
            synthesizer,
            output_stream,
            output_stream_handle,
            sink,
        }
    }

    pub async fn speak(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Err(_) = self.speak_internal(text).await {
            // Retry
            if let Err(e) = self.speak_internal(text).await {
                println!("TTS error: {}", e);
            }
        }
    }

    pub async fn speak_internal(&mut self, text: &str) -> anyhow::Result<()> {
        let response = self
            .synthesizer
            .synthesize_speech(SynthesizeSpeechRequest {
                input: Some(SynthesisInput {
                    input_source: Some(InputSource::Text(text.to_owned())),
                }),
                voice: Some(VoiceSelectionParams {
                    language_code: "en-us".to_string(),
                    name: VOICE.to_owned(),
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

        let data: Vec<u8> = response.audio_content;
        let cursor = Cursor::new(data);
        self.sink.append(rodio::Decoder::new(cursor)?);

        Ok(())
    }

    pub fn wait(&self) {
        self.sink.sleep_until_end();
    }
}
