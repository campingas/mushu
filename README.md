# mushu

A control surface on your phone for the AI coding agents running in [Ghostty](https://ghostty.org) and [Herdr](https://herdr.dev) on your own machines. Others sell this experience behind paywalls and closed code; mushu does it fully open source.

Your agents (Claude Code, Codex, OpenCode, Cursor CLI) keep running on your desktop or server. Your phone gets: a real terminal attached to the exact same live Herdr session as your desktop, an agent inbox, push notifications when an agent needs you, and one-tap approvals from anywhere (home wifi or any other network) over Tailscale.

No app store terminal, no sshd required, no third-party relay: each host runs a single `mushu-server` binary (Rust) that serves a PWA over Tailscale Serve, tailnet-only. Inspired by [t3code](https://github.com/pingdotgg/t3code)'s control-surface shape, built on Herdr instead of custom agent orchestration.

## Features

- Web terminal (xterm.js over WebSocket) attached to your persistent Herdr session: the phone and the desktop see the same panes, agents, and scrollback.
- Installable PWA with a touch toolbar (Esc, Tab, Ctrl, arrows, ^C) and aggressive reconnect: sessions survive network switches and phone sleep.
- Agent inbox: live status chips (working, blocked, done, idle) for every agent Herdr tracks.
- Push notifications on agent transitions (needs input, finished), end-to-end encrypted Web Push with no relay beyond Apple's push transit.
- Approvals and remote driving: tap an agent to approve, deny, send keys, or submit a full prompt, guarded against stale state and audit-logged.
- Privacy by construction: servers bind to loopback and are published tailnet-only through Tailscale Serve; nothing touches LAN, WAN, or third-party servers.

## Status

- [x] M0: repo scaffold and docs
- [x] M1: transport baseline (mosh fallback verified host-to-host over the tailnet)
- [x] M2: `mushu-server` with web terminal, validated from the phone on wifi, 4G, and 5G
- [x] M3: PWA install, agent inbox, Web Push, validated on a locked phone
- [x] M4: approvals from the phone, round-trip validated on 4G
- [ ] M5: polish and a reproducible setup for other Ghostty + Herdr users

See [docs/plan.md](docs/plan.md) for milestones and acceptance criteria, [docs/tasks.md](docs/tasks.md) for the checklist, [docs/architecture.md](docs/architecture.md) for the design, and [docs/decisions.md](docs/decisions.md) for decision records.

## Requirements (target setup)

- One or more hosts (macOS or Linux) running Herdr with agent integrations installed.
- A Tailscale tailnet joining hosts and phone, with Tailscale Serve available.
- iPhone or iPad on iOS 16.4+ (for PWA Web Push); any modern browser elsewhere.
- Optional: mosh on a Linux host as a raw-terminal fallback.

## License

GPLv3. Free software stays free: use it, ship it, but keep it open.
