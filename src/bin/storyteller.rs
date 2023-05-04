use async_openai::{
    types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
    Client,
};
use axum::{
    body::StreamBody,
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Router,
};
use futures::StreamExt;
use google_cognitive_apis::api::grpc::google::cloud::texttospeech::v1::{
    synthesis_input::InputSource, AudioConfig, AudioEncoding, SsmlVoiceGender, SynthesisInput,
    SynthesizeSpeechRequest, VoiceSelectionParams,
};
use google_cognitive_apis::texttospeech::synthesizer::Synthesizer;
use http::Method;
use serde::Deserialize;
use std::env;
use std::fs;
use tokio::io::{AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};

const VOICE: &str = "en-US-Studio-O";

#[tokio::main]
async fn main() {
    stackdriver_logger::init_with_cargo!("../../Cargo.toml");

    let mut port: u16 = 8080;
    match std::env::var("PORT") {
        Ok(p) => {
            match p.parse::<u16>() {
                Ok(n) => {
                    port = n;
                }
                Err(_e) => {}
            };
        }
        Err(_e) => {}
    };

    log::info!("Starting Storyteller");
    credentials().unwrap();

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);

    let router = {
        use axum::routing::get;
        Router::new()
            .route("/", get(index_get))
            .route("/audio", get(audio_get))
            .layer(cors)
            .layer(tower_http::trace::TraceLayer::new_for_http())
    };

    axum::Server::bind(&format!("0.0.0.0:{port}").parse().unwrap())
        .serve(router.into_make_service())
        .await
        .unwrap();
}

async fn index_get() -> Html<&'static str> {
    Html(include_str!("../../www/index.html"))
}

#[derive(Deserialize)]
struct AudioQuery {
    prompt: String,
}

async fn audio_get(query: Query<AudioQuery>) -> Result<impl IntoResponse, Error> {
    let (writer, reader) = tokio::io::duplex(1024);
    let prompt = query.prompt.clone();

    tokio::spawn(async move {
        if let Err(e) = stream_audio(prompt, writer).await {
            log::error!("stream_audio error: {}", e);
        }
    });

    let body = StreamBody::new(ReaderStream::new(reader));
    Ok((StatusCode::OK, body))
}

async fn stream_audio(prompt: String, audio_writer: DuplexStream) -> anyhow::Result<()> {
    let mut client = Client::new();
    let synthesizer = Synthesizer::create(credentials()?).await.unwrap();

    let (token_tx, token_rx) = mpsc::channel(100);

    tokio::spawn(async move {
        if let Err(e) = synthesize_task(synthesizer, token_rx, audio_writer).await {
            log::error!("synthesize_task error: {}", e);
        }
    });

    log::info!("Prompt: {:?}", prompt);

    query_gpt(
        &mut client,
        &[
            (
                Role::System,
                "\
You are a children's storyteller.
You tell stories based on Disney fairy tales that are suitable for a six-year-old.
Do not recite existing stories but make up a new one.
Any girls in the story should be intelligent and strong.
Do not summarize. Always finish with \"The End\"."
                    .to_string(),
            ),
            (Role::User, prompt),
        ],
        token_tx,
    )
    .await?;

    Ok(())
}

async fn query_gpt(
    client: &mut Client,
    messages: &[(Role, String)],
    token_tx: mpsc::Sender<String>,
) -> Result<(), anyhow::Error> {
    let mut send_messages = vec![];
    for (role, content) in messages {
        send_messages.push(
            ChatCompletionRequestMessageArgs::default()
                .content(content)
                .role(role.clone())
                .build()
                .unwrap(),
        )
    }

    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .max_tokens(1024u16)
        .messages(send_messages)
        .build()?;

    let mut stream = client.chat().create_stream(request).await?;
    let mut tokens = vec![];

    while let Some(result) = stream.next().await {
        let response = result?;
        for chat_choice in &response.choices {
            if let Some(ref content) = chat_choice.delta.content {
                tokens.push(content.clone());
                token_tx.send(content.clone()).await?;
            }
        }
    }

    log::info!("Response ({} tokens): {:?}", tokens.len(), tokens.join(""));

    Ok(())
}

async fn synthesize_task(
    mut synthesizer: Synthesizer,
    mut token_rx: mpsc::Receiver<String>,
    mut audio_writer: DuplexStream,
) -> anyhow::Result<()> {
    let mut wav_streamer = WavStreamer::new();
    let mut unspoken_text = String::new();

    while let Some(token) = token_rx.recv().await {
        unspoken_text.push_str(&token);
        if let Some(i) = find_break(&unspoken_text) {
            let new_text = unspoken_text.split_off(i + 1);
            if let Some(data) = synthesize(&mut synthesizer, &unspoken_text).await? {
                audio_writer.write_all(&wav_streamer.add(&data)).await?;
            }
            unspoken_text = new_text;
        }
    }

    if let Some(data) = synthesize(&mut synthesizer, &unspoken_text).await? {
        audio_writer.write_all(&wav_streamer.add(&data)).await?;
    }

    Ok(())
}

async fn synthesize(synthesizer: &mut Synthesizer, text: &str) -> anyhow::Result<Option<Vec<u8>>> {
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

struct WavStreamer {
    first: bool,
}

impl WavStreamer {
    fn new() -> Self {
        Self { first: true }
    }

    fn add(&mut self, data: &[u8]) -> Vec<u8> {
        let first = self.first;
        self.first = false;
        let mut result = Vec::new();
        if first {
            result.extend_from_slice(&data[..]);
            for i in 4..8 {
                result[i] = 0xff;
            }
            for i in 40..44 {
                result[i] = 0xff;
            }
        } else {
            result.extend_from_slice(&data[44..]);
        }
        result
    }
}

fn find_break(text: &str) -> Option<usize> {
    text.find(['.', '?', '!', '\n'].as_ref())
}

pub fn error(status_code: StatusCode, msg: String) -> Error {
    Error {
        status_code,
        err: anyhow::anyhow!(msg),
    }
}

pub struct Error {
    status_code: StatusCode,
    err: anyhow::Error,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.status_code, self.err.to_string()).into_response()
    }
}

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            err: err.into(),
        }
    }
}

fn credentials() -> Result<String, anyhow::Error> {
    let path = env::var("GOOGLE_APPLICATION_CREDENTIALS")?;
    Ok(fs::read_to_string(path)?)
}
