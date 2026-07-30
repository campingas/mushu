mod agents;
mod push;

use std::{io::Write, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(RustEmbed)]
#[folder = "../web"]
struct WebAssets;

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    shell_cmd: Arc<Vec<String>>,
    host: Arc<String>,
    push: push::PushStore,
}

#[derive(Deserialize)]
struct WsParams {
    token: String,
    #[serde(default = "default_cols")]
    cols: u16,
    #[serde(default = "default_rows")]
    rows: u16,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

#[derive(Deserialize)]
struct ControlMsg {
    resize: Option<Size>,
}

#[derive(Deserialize)]
struct Size {
    cols: u16,
    rows: u16,
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let bind: SocketAddr = std::env::var("MUSHU_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8422".into())
        .parse()
        .context("invalid MUSHU_BIND")?;
    let token = std::env::var("MUSHU_TOKEN").context("MUSHU_TOKEN is required")?;
    anyhow::ensure!(token.len() >= 16, "MUSHU_TOKEN must be at least 16 characters");
    let shell_cmd: Vec<String> = std::env::var("MUSHU_CMD")
        .unwrap_or_else(|_| "herdr".into())
        .split_whitespace()
        .map(String::from)
        .collect();
    anyhow::ensure!(!shell_cmd.is_empty(), "MUSHU_CMD must not be empty");

    let host = std::env::var("MUSHU_HOST").unwrap_or_else(|_| {
        std::process::Command::new("hostname")
            .arg("-s")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "host".into())
    });

    let push_store = push::PushStore::load_or_init()?;
    let state = AppState {
        token: Arc::new(token),
        shell_cmd: Arc::new(shell_cmd),
        host: Arc::new(host.clone()),
        push: push_store.clone(),
    };

    tokio::spawn(agents::notifier_loop(host, push_store));

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws_upgrade))
        .route("/api/agents", get(api_agents))
        .route("/api/action", post(api_action))
        .route("/push/vapid", get(push_vapid))
        .route("/push/subscribe", post(push_subscribe))
        .route("/push/test", post(push_test))
        .fallback(get(static_asset))
        .with_state(state);

    info!("mushu-server listening on {bind}");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn authed(headers: &HeaderMap, state: &AppState) -> bool {
    headers
        .get("x-mushu-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| constant_time_eq(t, &state.token))
}

async fn api_agents(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match agents::snapshot().await {
        Ok(s) => Json(serde_json::json!({
            "host": *state.host, "agents": s.agents, "workspaces": s.workspaces, "tabs": s.tabs
        }))
        .into_response(),
        // No herdr on this host: an empty inbox, not an error.
        Err(_) => Json(serde_json::json!({
            "host": *state.host, "agents": [], "workspaces": [], "tabs": []
        }))
        .into_response(),
    }
}

async fn api_action(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<agents::ActionRequest>,
) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match agents::run_action(&req).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(agents::ActionError::Gone) => (StatusCode::NOT_FOUND, "agent gone").into_response(),
        Err(agents::ActionError::Stale) => {
            (StatusCode::CONFLICT, "agent state changed").into_response()
        }
        Err(agents::ActionError::Invalid(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
        Err(agents::ActionError::Failed(e)) => {
            error!("action failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "action failed").into_response()
        }
    }
}

async fn push_vapid(State(state): State<AppState>) -> Response {
    Json(serde_json::json!({ "key": state.push.public_key_b64 })).into_response()
}

async fn push_subscribe(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(sub): Json<push::StoredSubscription>,
) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    state.push.subscribe(sub).await;
    StatusCode::NO_CONTENT.into_response()
}

async fn push_test(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    state
        .push
        .send_to_all("mushu test", "push notifications are working", &state.host)
        .await;
    StatusCode::NO_CONTENT.into_response()
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(state): State<AppState>,
) -> Response {
    if !constant_time_eq(&params.token, &state.token) {
        warn!("rejected ws connection: bad token");
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = terminal_session(socket, state, params.cols, params.rows).await {
            error!("terminal session ended with error: {e:#}");
        }
    })
}

async fn terminal_session(socket: WebSocket, state: AppState, cols: u16, rows: u16) -> Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(&state.shell_cmd[0]);
    cmd.args(&state.shell_cmd[1..]);
    cmd.env("TERM", "xterm-256color");
    // The attach must always look like a fresh outside client, even if
    // mushu-server itself was started from inside a Herdr session.
    for (key, _) in std::env::vars() {
        if key.starts_with("HERDR") {
            cmd.env_remove(&key);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        cmd.cwd(home);
    }
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut pty_reader = pair.master.try_clone_reader()?;
    let mut pty_writer = pair.master.take_writer()?;
    let master = pair.master;

    // pty reads are blocking; pump them through a channel from a blocking thread.
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    let reader_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match pty_reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (mut ws_tx, mut ws_rx) = socket.split();
    info!("terminal attached ({cols}x{rows})");

    loop {
        tokio::select! {
            chunk = out_rx.recv() => {
                match chunk {
                    Some(data) => ws_tx.send(Message::Binary(data.into())).await?,
                    None => break, // command exited
                }
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => pty_writer.write_all(&data)?,
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(ControlMsg { resize: Some(sz) }) = serde_json::from_str(&text) {
                            let _ = master.resize(PtySize {
                                rows: sz.rows,
                                cols: sz.cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        warn!("ws error: {e}");
                        break;
                    }
                }
            }
        }
    }

    let _ = child.kill();
    reader_task.abort();
    info!("terminal detached");
    Ok(())
}
