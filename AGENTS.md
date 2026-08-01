# AGENTS.md — mushu

mushu is a self-hosted control surface for AI coding agents: one Rust daemon per host serves a PWA over a Tailscale tailnet, giving a phone the same live [Herdr](https://herdr.dev) session as the desktop, an agent inbox, approvals, and Web Push. Agents working here are building a **security-sensitive daemon that people run on their own machines**, so correctness and honesty about what has actually been verified matter more than speed.

Start with [docs/development.md](docs/development.md) for the local loop and how to extend things, [docs/architecture.md](docs/architecture.md) for the shape, and [docs/decisions.md](docs/decisions.md) for why. This file is only the rules that are easy to get wrong.

## Layout

```
server/src/main.rs   routing, token auth, WebSocket terminal, static assets, `pair`
server/src/agents.rs Herdr snapshots, actions, the push notifier loop
server/src/push.rs   VAPID keypair, subscription store, Web Push sending
server/src/update.rs fixed-repository stable release checks and safe self-replacement
web/                 the PWA (index.html, app.js, style.css, sw.js, vendored xterm.js)
scripts/mushuctl     service control and phone pairing
services/            launchd and systemd unit templates
docs/                architecture, decisions, plan, tasks, development, mushuctl
```

## This repo is public

Never commit a token, host name, tailnet name, LAN address, or anything else tied to one person's machines. Docs describe the software, not a deployment: write "a Linux host", "a host whose 443 is already taken". Examples use obvious dummy values. A real token prefix once reached the README this way and had to be rotated.

## Invariants that look like mistakes

Each of these has cost real debugging time. Do not "clean them up" without reading the linked reasoning.

- **`web/manifest.webmanifest` has no `start_url`, deliberately.** iOS launches an installed web app at `start_url` when present, which discards the `#token` fragment the pairing QR carries, and the freshly installed app then prompts for a token (D11).
- **Static assets are served with `Cache-Control: no-cache`.** Without it browsers keep serving the pre-upgrade bundle after a deploy, and the change appears not to have shipped.
- **Web assets are embedded at compile time** by `rust-embed`. Editing anything in `web/` requires rebuilding the binary, and cargo does not always notice a change confined to `web/`.
- **Released binaries build with `--features vendored-tls`.** `web-push` reaches OpenSSL through `ece` on *every* platform, not only Linux, so a default macOS build links an absolute Homebrew dylib path that does not exist on a user's machine.
- **The client never navigates cross-origin.** Switching hosts swaps the WebSocket and API base URL in place (D9); navigating between origins inside an installed iOS app opens the in-app browser and breaks the layout.
- **Theme discovery happens before every terminal socket.** Keep `/api/host` authenticated, timeout-bounded, epoch-guarded, and limited to a normalized descriptor; never expose raw Herdr config or paths (D12).
- **iOS terminal dismissal suppresses xterm's compatibility `mousedown` before blurring.** Delayed blur refocuses the terminal; keep the capture-phase, touch-only guard and toolbar visual-viewport placement described in [development.md](docs/development.md#mobile-terminal-controls).

## Rules

- Every endpoint touching a session or a subscription authenticates with `authed()` (`x-mushu-token`, constant-time compare). Actions carry `state_change_seq` and return 409 when stale; keep that guard on anything new.
- Agent state comes from Herdr and nothing else (D5). New agent behaviour means a new `herdr` subcommand mapping in `agents.rs`, not new state tracking here.
- Agent-event notifications are sent from the notifier loop in `agents.rs`, so any suppression logic (quiet hours, rate limiting) has one home. `/push/test` calling `PushStore::send_to_all` directly is the deliberate exception: a test push must fire even when real events would be held back.
- Client state (`mushu_instances`, `mushu_active`, `mushu_vault`) is written through `saveInstances()`, which routes to the encrypted vault when the Face ID lock is on.
- Configuration is environment variables only, documented in the README table. No config file, no flags beyond `pair`.

## Working here

- Work on a branch with a conventional prefix (`feat/`, `fix/`, `docs/`, `chore/`). Merge with `--no-ff` after the change is validated.
- CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on **every branch push**. Keep them green locally first.
- Behaviour changes update the routed docs in the same change: `docs/plan.md` and `docs/tasks.md` for milestone state, `docs/decisions.md` for an architectural choice (add the next `D<n>`), `docs/development.md` for anything a future contributor would trip over.
- Milestones in `docs/plan.md` are each gated on explicit owner validation. Do not tick a gate yourself.
- Releases: bump `version` in `server/Cargo.toml` to match, then push a `v*` tag. Hosts install the published artifact via `install.sh` rather than a locally built binary, so what is running is always identifiable by hash.

## What cannot be verified from a terminal

iOS install and pairing, Face ID unlock, Web Push delivery, and anything about how the app feels on a phone are **owner-validated only**. Report them as untested rather than implying they passed. A browser check is evidence about the client logic, not about iOS.
