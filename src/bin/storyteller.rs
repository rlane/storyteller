use async_openai::{
    types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
    Client,
};
use axum::{
    body::StreamBody,
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
use std::env;
use std::fs;
use tokio::io::{AsyncWriteExt, DuplexStream};
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

pub async fn audio_get() -> Result<impl IntoResponse, Error> {
    let (writer, reader) = tokio::io::duplex(1024);

    tokio::spawn(async {
        if let Err(e) = stream_audio(writer).await {
            log::error!("stream_audio error: {}", e);
        }
    });

    let body = StreamBody::new(ReaderStream::new(reader));
    Ok((StatusCode::OK, body))
}

async fn stream_audio(audio_writer: DuplexStream) -> anyhow::Result<()> {
    let mut client = Client::new();
    let synthesizer = Synthesizer::create(credentials()?).await.unwrap();

    send_and_speak(
        &mut client,
        &[
        (
            Role::System,
            "You are a children's storyteller. You tell stories based on Disney fairy tales that are suitable for a four-year-old. Do not recite existing stories but make up a new one.
            The story should include a strong female protagonist."
                .to_string(),
        ),
        (
            Role::User,
            "Tell me a story.".to_string()
        ),
    ], synthesizer, audio_writer)
    .await?;

    Ok(())
}

async fn send_and_speak(
    client: &mut Client,
    messages: &[(Role, String)],
    mut synthesizer: Synthesizer,
    mut audio_writer: DuplexStream,
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

    let mut unspoken_text = String::new();
    let mut stream = client.chat().create_stream(request).await?;
    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                for chat_choice in &response.choices {
                    if let Some(ref content) = chat_choice.delta.content {
                        unspoken_text.push_str(content);
                        if let Some(i) = find_break(&unspoken_text) {
                            let new_text = unspoken_text.split_off(i + 1);
                            audio_writer
                                .write_all(&synthesize(&mut synthesizer, &unspoken_text).await)
                                .await?;
                            unspoken_text = new_text;
                        }
                    }
                }
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    audio_writer
        .write_all(&synthesize(&mut synthesizer, &unspoken_text).await)
        .await?;

    Ok(())
}

async fn synthesize(synthesizer: &mut Synthesizer, text: &str) -> Vec<u8> {
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
        .unwrap();

    let data: Vec<u8> = response.audio_content;
    data
}

fn find_break(mut text: &str) -> Option<usize> {
    const MAX_CHARS: usize = 1000;
    let force_break = text.len() > MAX_CHARS;
    if force_break {
        text = &text[0..MAX_CHARS];
        if let Some(i) = text.rfind(' ') {
            text = &text[0..i];
        } else {
            panic!("Failed to break text");
        }
    }

    if let Some(i) = text.rfind('\n') {
        return Some(i);
    } else if force_break {
        if let Some(i) = text.rfind(['.', '?', '!'].as_ref()) {
            return Some(i);
        }
        if let Some(i) = text.rfind([',', ';', ':', '"'].as_ref()) {
            return Some(i);
        }
        if let Some(i) = text.rfind(" and ") {
            return Some(i);
        }
        if let Some(i) = text.rfind([' '].as_ref()) {
            return Some(i);
        }
    }

    None
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
