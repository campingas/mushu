# Tasks

Flat checklist mirroring [plan.md](plan.md). Do not start a milestone before the previous one is validated.

Revised 2026-07-30 for the PWA pivot (decisions D6-D8). Dropped tasks from the Blink/ntfy plan are listed at the bottom for the record.

## M0: Repo scaffold and docs (done)

- [x] Rename initial branch to `main`.
- [x] Write README and docs: architecture, plan, tasks, decisions.
- [x] Gate: user validates docs and plan (2026-07-30).
- [x] Revise all docs for the PWA pivot (2026-07-30).

## M1: Transport baseline (done, scope revised)

- [x] Install mosh on the macOS host (brew).
- [x] Install mosh on the Linux host (apt); exposure governed by the local firewall, router NAT, and Tailscale Grants.
- [x] Verify mosh host to host over the tailnet: session established via MagicDNS, UDP verified on tailnet and LAN paths.
- [x] Wake a sleeping host via WoL magic packet; restore Tailscale afterwards (`--accept-routes` preserved).
- [x] Gate: pivot decision closes M1; phone-side verification moves to M2.

## M2: mushu-server MVP with web terminal

- [x] Scaffold Rust workspace: `server/` (axum, tokio, portable-pty) + `web/` (vendored xterm.js, embedded via rust-embed).
- [x] WebSocket terminal endpoint spawning the attach command in a pty (`herdr` by default, or a fallback such as `tmux new-session -A -s main` on a host without Herdr; HERDR* env stripped from the child to avoid nested-attach refusal).
- [x] Client: resize, reconnect with backoff, compact touch toolbar (Esc, Tab, Ctrl, ^C, disabled move placeholder, compose).
- [x] Bind hardened beyond plan: 127.0.0.1 only, published solely through Tailscale Serve; token auth (min 16 chars, constant-time compare).
- [x] Tailscale Serve: 443 where free, 8443 on a host whose 443 is taken; the pre-existing mapping verified untouched.
- [x] Service files: launchd `dev.mushu.server` (macOS), systemd user unit with linger (Linux); tokens in `~/.config/mushu-token` (600).
- [x] Verify from iPhone Safari: same session as desktop, works on wifi, 4G, and 5G with reconnect (user confirmed 2026-07-30).
- [x] Verify co-hosted services unaffected and the server unreachable from LAN (loopback bind); wss round-trip verified end to end.
- [x] Install Herdr on a host that lacked it and switch its MUSHU_CMD from tmux to herdr (2026-07-31).
- [x] Gate: user validates the phone terminal (2026-07-30).
- [x] Correct the keyboard-open layout so the fixed client frame follows the visual viewport, refits the terminal and remote PTY, and keeps the active prompt visible (2026-07-31).
- [x] Owner verified the keyboard-open correction in the installed iPhone PWA (2026-07-31); this does not reopen the passed M2 gate.
- [x] Implement compact mobile terminal controls and multiline compose with Clipboard API paste fallback (2026-08-01).
- [x] Owner verified in the installed iPhone PWA that the first terminal tap opens the keyboard, the second tap fully slides it out and leaves it dismissed, and the compact rounded toolbar sits 2px from the visual viewport bottom without the keyboard and immediately above the native iOS assistant area with it (2026-08-01).
- [ ] Owner verifies touch access to older Herdr scrollback, quick-key keyboard preservation, multiline compose, and six-control fit at 320px and 390px in the installed iPhone PWA; Safari's native accessory bar is not controllable by the app.

## M3: PWA, inbox, Web Push

- [x] PWA manifest, service worker, icons (rsvg-convert), home screen install support.
- [x] Inbox: header chips per agent (status, name, title) polling `/api/agents` backed by `herdr api snapshot`; empty on hosts without Herdr.
- [ ] Wire Claude Code hooks / Codex notify as low-latency triggers (deferred: 2s snapshot polling is fast enough for now).
- [x] Web Push: VAPID keypair auto-generated (`~/.config/mushu/vapid.key`), subscriptions persisted, encrypted sends, dead subscriptions pruned, and initial-snapshot replay suppressed; stable blocked incidents are debounced and latched per pane while working-to-done/idle transitions still notify (D13).
- [x] Notification taps focus or open the installed PWA on its own origin, queue the host/pane target through Face ID unlock, and switch to the exact saved instance in place; terminal context is fetched only afterward through authenticated `/api/attention`.
- [x] Verify latency and delivery with phone locked (user confirmed test push and live claude-finished push, 2026-07-30).
- [x] Gate: user validates notifications (2026-07-30).

## M4: Approvals from the phone

- [x] Action endpoint `/api/action`: keys (whitelisted names) via `herdr agent send-keys`, free text via `herdr agent prompt`, targeted by pane_id.
- [x] Buttons in inbox: tap a chip for the general action sheet. Notification attention cards use bounded current detection context and only expose numbered choices when 2-9 consecutive options starting at 1 are conservatively detected; otherwise they expose Open terminal, Deny/Esc, and Approve/Enter (D13).
- [x] Token auth, staleness guard (state_change_seq must match, else 409), audit log at `~/.config/mushu/actions.log`.
- [x] Server-side verification: 409 stale, 404 gone, 400 invalid, 204 success with audit entry (2026-07-30).
- [x] Verify round-trip from the phone on 4G (user confirmed 2026-07-30).
- [x] Gate: user validates the approval flow (2026-07-30).
- [x] Owner verified one notification for a real approval, deep-link routing through Face ID to the themed attention card, explicit choice submission, and the audited successful action in the installed iPhone PWA (2026-08-01).
- [ ] Owner verifies stale and already-resolved notification behavior in the installed iPhone PWA; terminal and desktop browser checks do not validate iOS Web Push lifecycle behavior.

## M5: Polish and shareability

Multi-host UX landed early on 2026-07-31: settings panel storing per-host URL and token, per-host alert toggles, drawer host switching from a single origin (D9), optional Face ID lock on stored tokens (D10).

- [x] `mushuctl pair`: QR sign-in with the token in the URL fragment (D11); public URL auto-discovered from the Tailscale Serve mapping that proxies our bind address.
- [x] Static assets served with `Cache-Control: no-cache`, so an upgraded server is not shadowed by a browser-cached bundle.
- [x] Docs truth-up: README pairing and multi-host sections including the shared VAPID keypair requirement; decisions D9-D11.
- [x] Prebuilt release binaries per platform plus CI running fmt, clippy, and `cargo test`. Four native-runner targets (macOS and Linux, x86_64 and aarch64); Linux builds use musl with OpenSSL vendored behind the `vendored-tls` feature, because web-push offers no rustls path. `install.sh` fetches and checksum-verifies the right asset.
- [x] Synchronize an adapted Herdr theme descriptor per host before every terminal connection, including host switches and reconnects (D12).
- [x] Replace the cog sheet with a themed full-screen safe-area Settings page while preserving per-host push controls and the global Face ID vault.
- [x] Make additional-host pairing QR-only with vendored jsQR camera/image decoding, authenticated `/api/host` validation, exact shared-VAPID enforcement, vault-aware saving, and media/secret cleanup.
- [x] Embed release tag/SHA/kind, expose `--version` and host build identity, and add authenticated cached/refreshable latest-stable status (D14).
- [x] Add the serialized fixed-repository installer: revalidate current latest stable, reject development/downgrade/stale/concurrent/arbitrary input, bound HTTPS downloads, verify exact platform checksum and staged identity, fsync, preserve `.previous`, atomically replace, cleanly shut down, and re-exec.
- [ ] Owner validates rear-camera and saved-image host pairing, VAPID-mismatch refusal, Face ID update re-authentication, confirmation copy, successful real-host update/reconnect, and failure behavior in the installed iPhone PWA.
- [ ] Owner verifies the remaining light, custom, and multi-host theme changes in the installed iPhone PWA; dark Catppuccin passed on 2026-08-01, while terminal/browser checks do not validate iOS chrome or perceived contrast.
- [ ] Quiet hours in the notifier loop.
- [ ] Limitations doc versus commercial alternatives and t3code.
- [ ] Gate: user validates and decides next direction.

## Dropped by the pivot (record only)

- Blink Shell install, key setup, phone mosh testing (D1 superseded).
- Self-hosted ntfy deployment and APNs upstream (D2 superseded, kept as possible fallback).
- Enabling Remote Login on a macOS host (explicitly out of scope; D8).
