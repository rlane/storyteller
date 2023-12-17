use axum::{
    body::StreamBody,
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Router,
};
use http::Method;
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};

use storyteller::stream_audio;

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

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);

    let router = {
        use axum::routing::get;
        Router::new()
            .route("/secret", get(index_get))
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
