# mushu

An open-source take on the [Moshi](https://getmoshi.app) experience for people who run their AI coding agents in [Ghostty](https://ghostty.org) and [Herdr](https://herdr.dev) on their own machines.

Your agents (Claude Code, Codex, OpenCode, Cursor CLI) keep running on your desktop or server. Your phone becomes a control surface: a web terminal attached to the exact same live Herdr session as your desktop, an agent inbox, push notifications when an agent needs you, and one-tap approvals from anywhere (home wifi or any other network) over Tailscale.

No app store terminal, no sshd required, no third-party relay: each host runs a single `mushu-server` binary (Rust) that serves a PWA over Tailscale Serve, tailnet-only. Inspired by [t3code](https://github.com/pingdotgg/t3code)'s control-surface shape, built on Herdr instead of custom agent orchestration.

## Why not just use Moshi

Moshi is a good app, but it is a paid, closed product, and its Pro tier gates the parts that matter most (mosh transport, multiplexer pairing). This project builds the same experience in the open, on top of tools you already run, with a stronger privacy posture (tailnet-only, E2E-encrypted push).

## How it maps to Moshi

| Moshi feature | mushu equivalent |
|---|---|
| Mobile terminal app | PWA served by `mushu-server`, xterm.js over WebSocket, installable to the home screen |
| SSH / mosh / ET transport | Tailscale + WebSocket reconnect; mosh kept as raw-terminal fallback |
| tmux session persistence | Herdr persistent sessions (literally the same session as desktop Ghostty) |
| moshi-hook agent events | `mushu-server` watching Herdr's socket API and agent hooks |
| Push notifications and inbox | iOS Web Push (VAPID, E2E encrypted), agent inbox in the PWA |
| Approvals from the phone | Inbox and notification actions driving `herdr agent send-keys` |

## Status

- [x] M0: repo scaffold and docs (revised for the PWA pivot)
- [x] M1: transport baseline (mosh fallback verified Mac to robrog over the tailnet)
- [ ] M2: `mushu-server` MVP with web terminal
- [ ] M3: PWA install, agent inbox, Web Push
- [ ] M4: approvals from the phone
- [ ] M5: polish and a reproducible setup for other Ghostty + Herdr users

See [docs/plan.md](docs/plan.md) for milestones and acceptance criteria, [docs/tasks.md](docs/tasks.md) for the checklist, [docs/architecture.md](docs/architecture.md) for the design, and [docs/decisions.md](docs/decisions.md) for decision records including the superseded Blink/ntfy path.

## Requirements (target setup)

- One or more hosts (macOS or Linux) running Herdr with agent integrations installed.
- A Tailscale tailnet joining hosts and phone, with Tailscale Serve available.
- iPhone or iPad on iOS 16.4+ (for PWA Web Push); any modern browser elsewhere.
- Optional: mosh on a Linux host as a raw-terminal fallback.

## License

MIT. mosh (optional fallback) is GPLv3 and used as an external tool, never redistributed here.
