use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::io::Cursor;

const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

#[allow(dead_code)]
pub struct Speaker {
    output_stream: rodio::OutputStream,
    output_stream_handle: rodio::OutputStreamHandle,
    sink: rodio::Sink,
}

impl Speaker {
    pub fn new() -> Self {
        let (output_stream, output_stream_handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&output_stream_handle).unwrap();
        Self {
            output_stream,
            output_stream_handle,
            sink,
        }
    }

    pub fn speak(&self, text: &str) {
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

    pub fn speak_internal(&self, text: &str) -> anyhow::Result<()> {
        let len = text.len();
        let text = utf8_percent_encode(text, FRAGMENT).to_string();

        let response = minreq::get(format!("https://translate.google.fr/translate_tts?ie=UTF-8&q={}&tl=en&total=1&idx=0&textlen={}&tl=en&client=tw-ob", text, len)).send()?;
        let data: Vec<u8> = response.as_bytes().to_vec();
        let cursor = Cursor::new(data);
        self.sink.append(rodio::Decoder::new(cursor)?);

        Ok(())
    }

    pub fn wait(&self) {
        self.sink.sleep_until_end();
    }
}
