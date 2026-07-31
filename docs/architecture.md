# Architecture

## Overview

Compute stays on the hosts. The phone gets a PWA (installable web app) served by each host over the tailnet: a real terminal attached to the same Herdr session the desktop uses, an agent inbox, approvals, and push notifications. No sshd, no app store terminal, no third-party relay.

Inspired by [t3code](https://github.com/pingdotgg/t3code) (MIT, local server plus mobile/web control surface), but self-hosted per machine, tailnet-only, and built on Herdr instead of its own agent orchestration.

```
iPhone / iPad (Safari PWA on home screen)
   |  Tailscale (any network: LAN, 4G, foreign wifi)
   |  https://<host>.<tailnet>.ts.net (Tailscale Serve, valid certs, tailnet-only)
   |
   |-- WebSocket --> mushu-server (Rust daemon, one per host: MacBook, robrog)
   |                   - serves the PWA assets
   |                   - web terminal: pty attach to Herdr sessions, streamed over WebSocket
   |                   - agent inbox: state from the Herdr socket API
   |                   - approval actions: mapped to herdr agent send-keys / prompt
   |                   - Web Push sender (VAPID) for agent events
   |
   |<-- Web Push (E2E encrypted, RFC 8291) -- "Claude needs approval on <host>"
   |-- tap notification --> PWA opens on the right host, session, and pending prompt
```

## Components

### mushu-server (Rust daemon, this repo)

One daemon per host, listening only on the host's Tailscale address. Responsibilities:

- Serve the PWA (static assets, single binary embed).
- Terminal endpoint: spawn `herdr session attach` (or `herdr agent attach`) in a pty per WebSocket connection, with auto-reconnect and resize handling on the client.
- Inbox endpoint: agent list and states from `herdr api snapshot` and `herdr agent wait` events.
- Action endpoint: approve/deny/prompt mapped to `herdr agent send-keys` / `herdr agent prompt`, with token auth, expiry for stale approvals, and an audit log.
- Web Push: hold subscriptions, send VAPID-signed encrypted notifications on agent events, with dedup and rate limiting.

### Front end: PWA

Web app installed to the iPhone home screen from Safari. xterm.js (or equivalent) terminal, agent inbox with per-host and per-agent views, approval buttons, service worker for Web Push (iOS 16.4+). The client reconnects WebSockets aggressively; roaming resilience comes from Herdr owning all session state, so a reconnect is cheap and lossless.

### Session persistence and agent state: Herdr

Herdr runs as a persistent server with attachable clients, so the phone and desktop Ghostty see the same panes, agents, and scrollback. Herdr's installed integrations (claude, codex, opencode, cursor) normalize agent state; mushu inherits multi-agent support instead of reimplementing detection. CLI surface relied on: `herdr agent list | get | read | wait | send-keys | prompt | attach`, `herdr api snapshot | schema`, `herdr session attach`.

### Transport: Tailscale + Tailscale Serve

Tailscale provides reachability from any network, MagicDNS, and encryption; verified from the iPhone over 4G. Tailscale Serve provides valid HTTPS certs (`<host>.<tailnet>.ts.net`), required for service workers and Web Push, while staying tailnet-only (no Funnel).

Constraint on robrog: Tailscale Serve already proxies Immich on 443. mushu-server must use a distinct serve port (for example `https://robrog.<tailnet>.ts.net:8443`) so the Immich mapping is untouched.

### Notifications: iOS Web Push

Sent by mushu-server directly through Apple's push service using VAPID. Payloads are end-to-end encrypted (RFC 8291), so APNs transit reveals nothing. No third-party relay, no extra app. Requires the PWA to be added to the home screen.

Multi-instance alerts: all hosts share one VAPID keypair (`~/.config/mushu/vapid.key` copied between hosts), so the PWA's single push subscription can be delivered to by any instance. The settings page stores instance URLs and tokens, and toggling alerts for an instance adds or removes this subscription in that instance's server-side store via `/push/subscribe`, `/push/unsubscribe`, and `/push/status` (all token-authed, CORS-enabled for cross-instance calls).

Single-origin client: one installed PWA connects to every saved instance from the origin it was installed from. Switching hosts swaps the WebSocket and API base URL plus token in place (cross-origin WebSocket and the CORS-enabled API); the page never navigates to another origin, so the iOS in-app browser overlay and its viewport bugs no longer occur.

### Fallback transport: mosh (installed, optional)

mosh 1.4.0 is installed on both hosts and verified Mac to robrog over the tailnet. It remains the raw-terminal fallback into robrog from any mosh-capable client if the web path is ever down. The Mac deliberately has Remote Login (sshd) off; it is reachable only through mushu-server on its tailnet address.

## Security model

- mushu-server binds only to the host's Tailscale address; nothing on LAN or WAN. Tailscale Grants stay least-privilege (self-owned devices only).
- No sshd on the Mac; the phone's only path into the Mac is mushu-server.
- Action endpoints authenticate (token at minimum) even though tailnet-only, because they execute keystrokes into live agent sessions; approvals expire and are audit-logged.
- Web Push payloads are E2E encrypted; no terminal content in notification payloads regardless.
- No credentials in this repo; the PWA holds its push subscription plus saved instance URLs and their access tokens on the phone. With the optional Face ID lock enabled, the tokens are AES-GCM encrypted at rest under a WebAuthn passkey PRF secret (Secure Enclave-backed, released only after Face ID / Touch ID); otherwise they live in plain localStorage.
