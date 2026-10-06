//! Serves the radar screen and streams the map to it over a WebSocket.

use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;
use tokio::sync::watch;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Screen;

pub async fn serve(addr: SocketAddr, map: watch::Receiver<String>) -> Result<()> {
    let app = Router::new().route("/ws", get(socket)).fallback(get(asset)).with_state(map);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("radar-detection is up → http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn socket(upgrade: WebSocketUpgrade, State(map): State<watch::Receiver<String>>) -> Response {
    upgrade.on_upgrade(move |ws| stream(ws, map))
}

async fn stream(mut ws: WebSocket, mut map: watch::Receiver<String>) {
    while map.changed().await.is_ok() {
        let json = map.borrow_and_update().clone();
        if ws.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }
}

async fn asset(uri: Uri) -> Response {
    let path = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        path => path,
    };
    let (path, file) = match Screen::get(path) {
        Some(file) => (path, file),
        None => match Screen::get("index.html") {
            Some(file) => ("index.html", file),
            None => {
                let hint = "the screen isn't built yet. run `pnpm build` in web/, or `pnpm dev` while working on it";
                return (StatusCode::NOT_FOUND, hint).into_response();
            }
        },
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.as_ref())], file.data).into_response()
}
