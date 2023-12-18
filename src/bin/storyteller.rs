use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Router,
};
use futures::StreamExt;
use http::Method;
use serde::Deserialize;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};

use storyteller::stream;

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
            .route("/websocket", get(websocket_handler))
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

async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket))
}

#[derive(Deserialize)]
struct WebsocketRequest {
    prompt: String,
}

async fn websocket(mut ws: WebSocket) {
    while let Some(Ok(msg)) = ws.next().await {
        if let Err(e) = handle_websocket_message(msg, &mut ws).await {
            log::error!("websocket error: {}", e);
            break;
        }
    }
}

async fn handle_websocket_message(msg: Message, sender: &mut WebSocket) -> anyhow::Result<()> {
    let text = match msg {
        Message::Text(request_json) => request_json,
        Message::Close(_) => {
            return Ok(());
        }
        _ => {
            anyhow::bail!("Expected text message");
        }
    };

    let request: WebsocketRequest = match serde_json::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            anyhow::bail!("Error parsing request: {}", e);
        }
    };

    let prompt = request.prompt;

    let (tx, mut rx) = mpsc::channel(10);
    tokio::spawn(async move {
        if let Err(e) = stream(prompt, tx).await {
            log::error!("stream error: {}", e);
        }
    });

    while let Some(chunk) = rx.recv().await {
        let response_json = serde_json::to_string(&chunk)?;
        sender.send(Message::Text(response_json)).await?;
    }

    Ok(())
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
