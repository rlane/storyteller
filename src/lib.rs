use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, Role,
    },
    Client,
};
use async_openai::{
    types::{CreateSpeechRequestArgs, SpeechModel, SpeechResponseFormat, Voice},
    Audio,
};
use futures::StreamExt;
use serde::{Serialize, Serializer};
use std::str;
use tokio::sync::mpsc;

const MODEL: &str = "gpt-4";
const MAX_TOKENS: u16 = 1024u16;

#[derive(Serialize)]
pub struct StoryChunk {
    text: String,
    audio: Base64,
}

pub async fn stream(prompt: String, chunk_tx: mpsc::Sender<StoryChunk>) -> anyhow::Result<()> {
    let client = Client::new();

    let (token_tx, token_rx) = mpsc::channel(100);

    {
        let client = client.clone();
        tokio::spawn(async move {
            if let Err(e) = synthesize_task(client, token_rx, chunk_tx).await {
                log::error!("synthesize_task error: {}", e);
            }
        });
    }

    log::info!("Prompt: {:?}", prompt);

    query_story(client, prompt, token_tx).await?;

    Ok(())
}

pub async fn query_story(
    mut client: Client<OpenAIConfig>,
    prompt: String,
    token_tx: mpsc::Sender<String>,
) -> Result<(), anyhow::Error> {
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
Do not summarize. Always finish with \"The End\".
Put a tilde (~) after each sentence."
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
    client: &mut Client<OpenAIConfig>,
    messages: &[(Role, String)],
    token_tx: mpsc::Sender<String>,
) -> Result<(), anyhow::Error> {
    let mut send_messages = vec![];
    for (role, content) in messages {
        send_messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(content.as_str())
                .role(role.clone())
                .build()
                .unwrap(),
        ))
    }

    let request = CreateChatCompletionRequestArgs::default()
        .model(MODEL)
        .max_tokens(MAX_TOKENS)
        .messages(send_messages.as_slice())
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

    log::info!(
        "Response ({} tokens, {} characters): {:?}",
        tokens.len(),
        tokens.join("").len(),
        tokens.join("")
    );

    Ok(())
}

async fn synthesize_task(
    client: Client<OpenAIConfig>,
    mut token_rx: mpsc::Receiver<String>,
    chunk_tx: mpsc::Sender<StoryChunk>,
) -> anyhow::Result<()> {
    let mut unspoken_text = String::new();

    while let Some(token) = token_rx.recv().await {
        unspoken_text.push_str(&token);
        if let Some(i) = find_break(&unspoken_text) {
            let new_text = unspoken_text.split_off(i + 1);
            if unspoken_text.ends_with('~') {
                unspoken_text.pop();
            }
            if let Some(data) = synthesize(&client, &unspoken_text).await? {
                chunk_tx
                    .send(StoryChunk {
                        text: unspoken_text,
                        audio: Base64(data),
                    })
                    .await?;
            }
            unspoken_text = new_text;
        }
    }

    if let Some(data) = synthesize(&client, &unspoken_text).await? {
        chunk_tx
            .send(StoryChunk {
                text: unspoken_text,
                audio: Base64(data),
            })
            .await?;
    }

    Ok(())
}

async fn synthesize(client: &Client<OpenAIConfig>, text: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }

    log::trace!("Synthesizing {:?}", &text);
    let start_time = std::time::Instant::now();

    let speech_model = SpeechModel::Tts1;
    let voice = Voice::Nova;
    let request = CreateSpeechRequestArgs::default()
        .model(speech_model)
        .voice(voice)
        .input(text)
        .response_format(SpeechResponseFormat::Flac)
        .build()?;

    let audio = Audio::new(client);
    let response = audio.speech(request).await?;

    let data: Vec<u8> = response.bytes.into();

    log::trace!(
        "synthesize_speech took {}ms, {} bytes input, {} bytes output",
        start_time.elapsed().as_millis(),
        text.len(),
        data.len()
    );
    Ok(Some(data))
}

fn find_break(text: &str) -> Option<usize> {
    text.find(['~', '\n'].as_ref())
}

#[derive(Debug)]
pub struct Base64(Vec<u8>);
impl Serialize for Base64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&base64::display::Base64Display::new(
            &self.0,
            &base64::engine::general_purpose::STANDARD,
        ))
    }
}
