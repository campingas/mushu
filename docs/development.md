# Development

How to build, run, and extend mushu. See [architecture.md](architecture.md) for what the pieces are and [decisions.md](decisions.md) for why they are that way.

## Layout

| Path | Contents |
|---|---|
| `server/src/main.rs` | routing, auth, WebSocket terminal, static assets, the `pair` subcommand |
| `server/src/agents.rs` | Herdr snapshot parsing, actions, and the push notifier loop |
| `server/src/push.rs` | VAPID keypair, subscription store, Web Push sending |
| `web/` | the PWA: `index.html`, `app.js`, `style.css`, `sw.js`, vendored xterm.js |
| `scripts/mushuctl` | service control and phone pairing |
| `services/` | launchd and systemd unit templates |

## Local loop

Run a throwaway instance instead of touching your real service. A plain shell as `MUSHU_CMD` keeps Herdr out of the picture while working on the client:

```sh
cargo build --release
MUSHU_BIND=127.0.0.1:8498 MUSHU_HOST=alpha \
  MUSHU_TOKEN=devtokendevtoken MUSHU_CMD=/bin/sh \
  ./target/release/mushu-server
```

Open `http://127.0.0.1:8498/#devtokendevtoken`: the fragment signs you in, so no token prompt. Plain HTTP is fine locally, but service workers and Web Push need HTTPS, so notifications can only be exercised through Tailscale Serve.

To work on multi-host behaviour, start a second instance on another port with a different `MUSHU_HOST`, then add it from the settings panel. Host switching, per-host alert toggles, and the drawer all work against two local instances.

**The web assets are embedded into the binary** at compile time by `rust-embed` (`#[derive(RustEmbed)] #[folder = "../web"]`). Editing anything in `web/` therefore requires rebuilding the binary, and cargo does not always notice a change confined to `web/`; touch a source file or `cargo clean -p mushu-server` if a change seems not to apply. Assets are served with `Cache-Control: no-cache` because without it browsers kept serving the pre-upgrade bundle after a deploy.

`web/manifest.webmanifest` deliberately has **no `start_url`**. Do not add one back: iOS launches an installed web app at `start_url` when it is present, which discards the `#token` fragment that `mushuctl pair` relies on, and the freshly installed app then prompts for a token. With the key absent, `start_url` defaults to the document URL that was added to the home screen, so the fragment survives the install.

## Extending

Adding an endpoint: register it on the router in `main.rs`, and gate anything sensitive with the existing `authed(&headers, &state)` helper, which compares the `x-mushu-token` header in constant time. The `CorsLayer` already allows that header cross-origin, which is what lets one installed app drive every host (D9); a new endpoint the client calls on *other* instances needs nothing extra, but one that must never be called cross-origin should be excluded from the layer.

Agent state comes from Herdr and nothing else (D5). New agent-facing behaviour usually means a new action in `agents.rs` mapped onto a `herdr` subcommand, not new state tracking here. Actions carry `state_change_seq` and are rejected with 409 when stale, so any new action should keep that guard.

Notifications are sent from the notifier loop in `agents.rs`, which polls every two seconds and fires only on status transitions. Send through the loop rather than calling `PushStore::send_to_all` from a new place, so that suppression logic stays in one spot.

Client state lives in `localStorage`: `mushu_instances` (host URLs and tokens), `mushu_active`, and `mushu_vault` when the Face ID lock is on. Anything added there must go through `saveInstances()`, which writes to the encrypted vault when it is enabled.

## TLS constraint

`web-push` reaches OpenSSL through its `ece` dependency on **every** platform, not only Linux, and offers no rustls option. Default builds therefore link the system OpenSSL, which on macOS means an absolute Homebrew path that does not exist on other machines. Released binaries are built with `--features vendored-tls`, which compiles OpenSSL from source and links it statically. Local development does not need the feature; anything producing a binary for someone else does.

## Releasing

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on every branch push. Keep those green locally before pushing.

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds four targets (macOS and Linux, x86_64 and aarch64), publishes them with `mushuctl` and a `SHA256SUMS` file, and generates release notes. Both macOS binaries are built on the arm64 runner, the x86_64 one cross-compiled, because the Intel runner has been retired. The workflow also accepts `workflow_dispatch`, which runs the builds without publishing anything: the release job is gated on the ref being a tag.

`install.sh` resolves `releases/latest`, so it only works once a release exists.
