mod agents;
mod push;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::sync::{mpsc, watch, Notify};
use tower_http::cors::{Any, CorsLayer};
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
    shutdown: watch::Receiver<bool>,
    sessions: SessionTracker,
}

#[derive(Clone, Default)]
struct SessionTracker {
    active: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}

impl SessionTracker {
    fn enter(&self) -> SessionGuard {
        self.active.fetch_add(1, Ordering::AcqRel);
        SessionGuard(self.clone())
    }

    async fn wait_empty(&self) {
        loop {
            let notified = self.notify.notified();
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

struct SessionGuard(SessionTracker);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.notify.notify_waiters();
        }
    }
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
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn load_token(token_file: Option<OsString>, token: Option<String>) -> Result<String> {
    let token = match token_file {
        Some(path) => {
            let path = Path::new(&path);
            let mut file = File::open(path)
                .with_context(|| format!("failed to read MUSHU_TOKEN_FILE {}", path.display()))?;
            let metadata = file.metadata().with_context(|| {
                format!("failed to inspect MUSHU_TOKEN_FILE {}", path.display())
            })?;
            anyhow::ensure!(
                metadata.is_file(),
                "MUSHU_TOKEN_FILE must be a regular file"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                anyhow::ensure!(
                    metadata.mode() & 0o077 == 0,
                    "MUSHU_TOKEN_FILE must not be accessible by group or other users"
                );
            }
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .with_context(|| format!("failed to read MUSHU_TOKEN_FILE {}", path.display()))?;
            contents.trim().to_string()
        }
        None => token.context("MUSHU_TOKEN or MUSHU_TOKEN_FILE is required")?,
    };
    anyhow::ensure!(
        token.len() >= 16,
        "MUSHU_TOKEN must be at least 16 characters"
    );
    Ok(token)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let bind: SocketAddr = std::env::var("MUSHU_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8422".into())
        .parse()
        .context("invalid MUSHU_BIND")?;
    let token = load_token(
        std::env::var_os("MUSHU_TOKEN_FILE"),
        std::env::var("MUSHU_TOKEN").ok(),
    )?;
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
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let sessions = SessionTracker::default();
    let state = AppState {
        token: Arc::new(token),
        shell_cmd: Arc::new(shell_cmd),
        host: Arc::new(host.clone()),
        push: push_store.clone(),
        shutdown: shutdown_rx.clone(),
        sessions: sessions.clone(),
    };

    let notifier_task = tokio::spawn(agents::notifier_loop(host, push_store, shutdown_rx));

    // Other mushu instances' PWAs call /push/* and /api/agents cross-origin;
    // auth still comes from the x-mushu-token header on every sensitive route.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, HeaderName::from_static("x-mushu-token")]);

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws_upgrade))
        .route("/api/agents", get(api_agents))
        .route("/api/action", post(api_action))
        .route("/push/vapid", get(push_vapid))
        .route("/push/subscribe", post(push_subscribe))
        .route("/push/unsubscribe", post(push_unsubscribe))
        .route("/push/status", post(push_status))
        .route("/push/test", post(push_test))
        .layer(cors)
        .fallback(get(static_asset))
        .with_state(state);

    info!("mushu-server listening on {bind}");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let signal_tx = shutdown_tx.clone();
    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            info!("shutdown signal received");
            let _ = signal_tx.send(true);
        })
        .await;
    let _ = shutdown_tx.send(true);
    sessions.wait_empty().await;
    notifier_task.await.context("notifier task failed")?;
    server_result?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.expect("failed to install SIGINT handler");
            }
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
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

#[derive(Deserialize)]
struct EndpointReq {
    endpoint: String,
}

async fn push_unsubscribe(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<EndpointReq>,
) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if state.push.unsubscribe(&req.endpoint).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, "not subscribed").into_response()
    }
}

async fn push_status(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<EndpointReq>,
) -> Response {
    if !authed(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    Json(serde_json::json!({ "subscribed": state.push.is_subscribed(&req.endpoint).await }))
        .into_response()
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
    let session = state.sessions.enter();
    ws.on_upgrade(move |socket| async move {
        let _session = session;
        if let Err(e) = terminal_session(socket, state, params.cols, params.rows).await {
            error!("terminal session ended with error: {e:#}");
        }
    })
}

async fn terminal_session(socket: WebSocket, state: AppState, cols: u16, rows: u16) -> Result<()> {
    let mut shutdown = state.shutdown.clone();
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
            _ = wait_for_shutdown(&mut shutdown) => break,
            chunk = out_rx.recv() => {
                match chunk {
                    Some(data) => {
                        if let Err(e) = ws_tx.send(Message::Binary(data.into())).await {
                            warn!("ws send error: {e}");
                            break;
                        }
                    }
                    None => break, // command exited
                }
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if let Err(e) = pty_writer.write_all(&data) {
                            warn!("pty write error: {e}");
                            break;
                        }
                    }
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
    let _ = child.wait();
    drop(pty_writer);
    drop(master);
    let _ = reader_task.await;
    info!("terminal detached");
    Ok(())
}

pub(crate) async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.changed().await;
}

#[cfg(test)]
mod tests {
    use super::load_token;
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);

    impl TestFile {
        fn new(contents: &str) -> Self {
            let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("mushu-token-test-{}-{id}", std::process::id()));
            fs::write(&path, contents).expect("write token test file");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                    .expect("secure token test file");
            }
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn token_file_takes_precedence_and_is_trimmed() {
        let file = TestFile::new("  file-token-1234567890 \n");
        let token = load_token(
            Some(file.path().as_os_str().to_owned()),
            Some("environment-token-1234".into()),
        )
        .expect("load file token");
        assert_eq!(token, "file-token-1234567890");
    }

    #[test]
    fn missing_token_file_does_not_fall_back_to_environment() {
        let missing =
            std::env::temp_dir().join(format!("mushu-token-missing-{}", std::process::id()));
        let error = load_token(
            Some(missing.into_os_string()),
            Some("environment-token-1234".into()),
        )
        .expect_err("missing configured file must fail");
        assert!(error
            .to_string()
            .contains("failed to read MUSHU_TOKEN_FILE"));
    }

    #[test]
    fn invalid_token_file_does_not_fall_back_to_environment() {
        let dir = std::env::temp_dir();
        let error = load_token(
            Some(dir.into_os_string()),
            Some("environment-token-1234".into()),
        )
        .expect_err("unreadable configured file must fail");
        assert!(error.to_string().contains("must be a regular file"));
    }

    #[test]
    fn token_minimum_length_applies_to_file_and_environment() {
        let file = TestFile::new("too-short\n");
        let file_error = load_token(Some(OsString::from(file.path())), None)
            .expect_err("short file token must fail");
        let env_error =
            load_token(None, Some("too-short".into())).expect_err("short env token must fail");
        assert!(file_error.to_string().contains("at least 16 characters"));
        assert!(env_error.to_string().contains("at least 16 characters"));
    }

    #[test]
    fn environment_token_remains_supported() {
        let token = load_token(None, Some("environment-token-1234".into()))
            .expect("load environment token");
        assert_eq!(token, "environment-token-1234");
    }

    #[cfg(unix)]
    #[test]
    fn token_file_rejects_group_or_other_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let file = TestFile::new("file-token-1234567890\n");
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o644))
            .expect("relax token test permissions");
        let error = load_token(Some(file.path().as_os_str().to_owned()), None)
            .expect_err("insecure token permissions must fail");
        assert!(error
            .to_string()
            .contains("must not be accessible by group or other users"));
    }
}
