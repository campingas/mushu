# Plan

Each milestone ends with a validation gate: the user reviews and explicitly approves before the next milestone starts. No step jumps ahead of its gate.

Revised 2026-07-30: pivot from Blink Shell + ntfy to a self-hosted PWA with Web Push (see decisions D6-D8). M1 was completed under the original plan and its mosh work is kept as the fallback transport.

## M0: Repo scaffold and docs (done)

README and docs created, initial branch renamed to `main`. Docs revised for the PWA pivot the same day. Gate passed 2026-07-30.

## M1: Transport baseline (done, scope revised)

Completed: mosh 1.4.0 installed on the Mac (brew) and robrog (apt); mosh session verified Mac to robrog over the tailnet via MagicDNS; UDP paths verified; Tailscale restored on the Mac. robrog woken by Wake-on-LAN.

Scope change: the phone terminal is now the PWA, so Blink Shell setup and phone mosh testing are dropped. mosh remains the raw-terminal fallback into robrog. The Mac keeps Remote Login off by explicit owner decision.

Gate: passed implicitly by the pivot decision; remaining phone verification moves to M2.

## M2: mushu-server MVP with web terminal (done)

Goal: from Safari on the iPhone, on any network, open the same live Herdr session as the desktop.

Steps:

1. Scaffold the Rust workspace: `server/` crate (axum + tokio + portable-pty) and `web/` front end (vanilla TS + xterm.js, embedded in the binary).
2. Terminal endpoint: WebSocket that spawns `herdr session attach` in a pty; client handles resize, reconnect with backoff, and touch keyboard basics (Ctrl, Esc, Tab, arrows toolbar).
3. Bind strictly to the host's Tailscale address; token auth on all endpoints.
4. Configure Tailscale Serve: Mac on 443, robrog on a distinct port (8443) to leave the existing Immich mapping on 443 untouched.
5. Run on both hosts (launchd on macOS, systemd on robrog).

Acceptance criteria:

- iPhone Safari opens `https://<host>.<tailnet>.ts.net[:port]`, attaches, and shows the same Herdr session as desktop Ghostty (action on one visible on the other).
- Works on home wifi and on 4G; a wifi-to-4G switch recovers the session within seconds via reconnect, with no lost Herdr state.
- Immich on robrog remains reachable at its existing URL.
- Server unreachable from LAN and WAN addresses (tailnet bind verified).

Gate: passed 2026-07-30, validated from the phone on wifi, 4G, and 5G.

## M3: PWA install, agent inbox, and Web Push (done)

Goal: mushu on the home screen, agent events as push notifications.

Steps:

1. PWA manifest, service worker, home screen install flow.
2. Inbox view: agents across hosts with states, from `herdr api snapshot` plus `herdr agent wait` events; Claude Code hooks / Codex notify wired in where they beat polling latency.
3. Web Push: VAPID keys, subscription storage, encrypted sends on agent events (waiting for approval, turn finished, error), dedup and rate limiting.
4. Notification tap opens the PWA focused on the right host, agent, and session.

Acceptance criteria:

- An agent hitting a permission prompt or finishing a turn on either host produces a phone notification within a few seconds, phone locked, on 4G.
- Tap lands in the PWA with the relevant session visible.
- No terminal content in push payloads; payloads E2E encrypted.

Gate: passed 2026-07-30, test and live pushes delivered to a locked phone.

## M4: Approvals from the phone (done)

Goal: act on an agent directly from the inbox or notification.

Steps:

1. Action endpoint mapping approve/deny/custom prompt to `herdr agent send-keys` / `herdr agent prompt` on the correct pane.
2. Approve/deny buttons in the inbox and on notifications (iOS notification actions where supported, else one tap into the PWA action sheet).
3. Guard rails: token auth, expiry for stale approvals, audit log of phone-initiated actions.

Acceptance criteria:

- Full approval round-trip from the iPhone on 4G.
- A stale or replayed approval is rejected; every action is logged.

Gate: passed 2026-07-30, approval round-trip validated on 4G.

## M5: Polish and shareability

Goal: any Ghostty + Herdr user can reproduce this stack from the repo.

Steps:

1. Install story: single binary release, setup script or step-by-step doc covering hosts, tailnet, Serve, PWA install, and service files.
2. Inbox refinement: grouping, priorities, quiet hours, multi-host UX.
3. Honest limitations doc versus commercial alternatives (voice input, image paste, Live Activities out of scope without a native app) and versus t3code.

Acceptance criteria:

- A third party following the docs alone can reach the M4 experience.
- Limitations and non-goals are documented.

Gate: user validates and decides what comes next.
