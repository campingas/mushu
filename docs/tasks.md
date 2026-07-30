# Tasks

Flat checklist mirroring [plan.md](plan.md). Do not start a milestone before the previous one is validated.

Revised 2026-07-30 for the PWA pivot (decisions D6-D8). Dropped tasks from the Blink/ntfy plan are listed at the bottom for the record.

## M0: Repo scaffold and docs (done)

- [x] Rename initial branch to `main`.
- [x] Write README and docs: architecture, plan, tasks, decisions.
- [x] Gate: user validates docs and plan (2026-07-30).
- [x] Revise all docs for the PWA pivot (2026-07-30).

## M1: Transport baseline (done, scope revised)

- [x] Install mosh on the macOS host (brew): mosh 1.4.0_40.
- [x] Install mosh on the Linux host (apt): mosh 1.4.0 on robrog, ufw inactive, exposure governed by router NAT and Tailscale Grants.
- [x] Verify mosh host-to-host over tailnet: Mac to robrog session established via MagicDNS, UDP verified on tailnet and LAN paths.
- [x] Wake robrog via WoL magic packet; restore Tailscale on the Mac (`--accept-routes` preserved).
- [x] Gate: pivot decision closes M1; phone-side verification moves to M2.

## M2: mushu-server MVP with web terminal

- [x] Scaffold Rust workspace: `server/` (axum, tokio, portable-pty) + `web/` (vendored xterm.js, embedded via rust-embed).
- [x] WebSocket terminal endpoint spawning the attach command in a pty (`herdr` on the Mac, `tmux new-session -A -s rob` on robrog since Herdr is not installed there yet; HERDR* env stripped from the child to avoid nested-attach refusal).
- [x] Client: resize, reconnect with backoff, touch toolbar (Ctrl, Esc, Tab, arrows, ^C).
- [x] Bind hardened beyond plan: 127.0.0.1 only, published solely through Tailscale Serve; token auth (min 16 chars, constant-time compare).
- [x] Tailscale Serve: Mac on 443, robrog on 8443, Immich verified untouched on 443.
- [x] Service files: launchd `dev.mushu.server` (Mac), systemd user unit with linger (robrog); tokens in `~/.config/mushu-token` (600).
- [x] Verify from iPhone Safari: same session as desktop, works on wifi, 4G, and 5G with reconnect (user confirmed 2026-07-30).
- [x] Verify Immich unaffected and server unreachable from LAN (loopback bind); wss round-trip to robrog tmux verified end to end.
- [ ] Install Herdr on robrog and switch its MUSHU_CMD from tmux to herdr.
- [x] Gate: user validates the phone terminal (2026-07-30).

## M3: PWA, inbox, Web Push

- [x] PWA manifest, service worker, icons (rsvg-convert), home screen install support.
- [x] Inbox: header chips per agent (status, name, title) polling `/api/agents` backed by `herdr api snapshot`; empty on hosts without Herdr.
- [ ] Wire Claude Code hooks / Codex notify as low-latency triggers (deferred: 2s snapshot polling is fast enough for now).
- [x] Web Push: VAPID keypair auto-generated (`~/.config/mushu/vapid.key`), subscriptions persisted, encrypted sends, dead subscriptions pruned, initial-snapshot replay suppressed; notifies on transitions to blocked and working-to-done/idle.
- [x] Notification tap focuses or opens the PWA (per-host/session deep link deferred to M4 alongside actions).
- [x] Verify latency and delivery with phone locked (user confirmed test push and live claude-finished push, 2026-07-30).
- [x] Gate: user validates notifications (2026-07-30).

## M4: Approvals from the phone

- [x] Action endpoint `/api/action`: keys (whitelisted names) via `herdr agent send-keys`, free text via `herdr agent prompt`, targeted by pane_id.
- [x] Buttons in inbox: tap a chip for the action sheet (Approve=enter, Esc, y/n/1/2/3, prompt field). iOS Web Push has no action buttons, so notification tap opens the PWA and the sheet is one tap away.
- [x] Token auth, staleness guard (state_change_seq must match, else 409), audit log at `~/.config/mushu/actions.log`.
- [x] Server-side verification: 409 stale, 404 gone, 400 invalid, 204 success with audit entry (2026-07-30).
- [x] Verify round-trip from the phone on 4G (user confirmed 2026-07-30).
- [x] Gate: user validates the approval flow (2026-07-30).

## M5: Polish and shareability

- [ ] Single binary release and setup docs/script.
- [ ] Inbox grouping, priorities, quiet hours, multi-host UX.
- [ ] Limitations doc versus Moshi and t3code.
- [ ] Gate: user validates and decides next direction.

## Dropped by the pivot (record only)

- Blink Shell install, key setup, phone mosh testing (D1 superseded).
- Self-hosted ntfy deployment and APNs upstream (D2 superseded, kept as possible fallback).
- Enabling Remote Login on the Mac (explicitly refused; D8).
