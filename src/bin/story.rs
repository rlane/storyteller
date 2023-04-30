use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
        CreateChatCompletionRequestArgs, Role,
    },
    Client,
};
use clap::Parser;
use futures::StreamExt;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::io::{Cursor, Write};

const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Prompt.
    #[arg(short, long, default_value = "Tell me a story.")]
    prompt: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut chat = Chat::new(Client::new());
    chat.send_and_speak(&[
        (
            Role::System,
            "You are a children's storyteller. You tell stories based on Disney fairy tales that are suitable for a four-year-old. Do not recite existing stories but make up a new one.
            The story should include a strong female protagonist."
                .to_string(),
        ),
        (
            Role::User,
            args.prompt
        ),
    ])
    .await?;

    Ok(())
}

#[allow(dead_code)]
struct Speaker {
    output_stream: rodio::OutputStream,
    output_stream_handle: rodio::OutputStreamHandle,
    sink: rodio::Sink,
}

impl Speaker {
    fn new() -> Self {
        let (output_stream, output_stream_handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&output_stream_handle).unwrap();
        Self {
            output_stream,
            output_stream_handle,
            sink,
        }
    }

    fn speak(&self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if text.len() > 100 {
            println!("Text was too long!");
            return;
        }
        if let Err(_) = self.speak_internal(text) {
            // Retry
            if let Err(e) = self.speak_internal(text) {
                println!("TTS error: {}", e);
            }
        }
    }

    fn speak_internal(&self, text: &str) -> anyhow::Result<()> {
        let len = text.len();
        let text = utf8_percent_encode(text, FRAGMENT).to_string();

        let response = minreq::get(format!("https://translate.google.fr/translate_tts?ie=UTF-8&q={}&tl=en&total=1&idx=0&textlen={}&tl=en&client=tw-ob", text, len)).send()?;
        let data: Vec<u8> = response.as_bytes().to_vec();
        let cursor = Cursor::new(data);
        self.sink.append(rodio::Decoder::new(cursor)?);

        Ok(())
    }

    fn wait(&self) {
        self.sink.sleep_until_end();
    }
}

struct Chat {
    client: Client,
    speaker: Speaker,
    history: Vec<(Role, String)>,
}

impl Chat {
    fn new(client: Client) -> Self {
        Self {
            client,
            speaker: Speaker::new(),
            history: Vec::new(),
        }
    }

    async fn send_and_speak(&mut self, messages: &[(Role, String)]) -> Result<(), anyhow::Error> {
        for (role, content) in messages {
            println!("{:?}: {}", role, content);
            self.history.push((role.clone(), content.clone()));
        }

        let mut send_messages = vec![];
        for (role, content) in &self.history {
            send_messages.push(message(role.clone(), content));
        }

        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-3.5-turbo")
            .max_tokens(1024u16)
            .messages(send_messages)
            .build()?;

        let mut unspoken_text = String::new();
        let mut stream = self.client.chat().create_stream(request).await?;
        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    response.choices.iter().for_each(|chat_choice| {
                        if let Some(ref content) = chat_choice.delta.content {
                            std::io::stdout().write_all(content.as_bytes()).unwrap();
                            unspoken_text.push_str(content);
                            if let Some(i) = find_break(&unspoken_text) {
                                let new_text = unspoken_text.split_off(i + 1);
                                self.speaker.speak(&unspoken_text);
                                unspoken_text = new_text;
                            }
                        }
                    });
                    std::io::stdout().flush().unwrap();
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }

        self.speaker.speak(&unspoken_text);
        self.speaker.wait();
        println!();

        Ok(())
    }
}

fn message(role: Role, content: &str) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessageArgs::default()
        .content(content)
        .role(role)
        .build()
        .unwrap()
}

fn find_break(mut text: &str) -> Option<usize> {
    const MAX_CHARS: usize = 100;
    let force_break = text.len() > MAX_CHARS;
    if force_break {
        text = &text[0..MAX_CHARS];
        if let Some(i) = text.rfind(' ') {
            text = &text[0..i];
        } else {
            panic!("Failed to break text");
        }
    }

    if let Some(i) = text.rfind(['.', '?', '!', '\n'].as_ref()) {
        return Some(i);
    } else if force_break {
        if let Some(i) = text.rfind([',', ';', ':', '"'].as_ref()) {
            return Some(i);
        }
        if let Some(i) = text.rfind([' '].as_ref()) {
            return Some(i);
        }
    }

    None
}
