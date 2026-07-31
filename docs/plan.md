# Plan

Each milestone ends with a validation gate: the user reviews and explicitly approves before the next milestone starts. No step jumps ahead of its gate.

Revised 2026-07-30: pivot from Blink Shell + ntfy to a self-hosted PWA with Web Push (see decisions D6-D8). M1 was completed under the original plan and its mosh work is kept as the fallback transport.

## M0: Repo scaffold and docs (done)

README and docs created, initial branch renamed to `main`. Docs revised for the PWA pivot the same day. Gate passed 2026-07-30.

## M1: Transport baseline (done, scope revised)

Completed: mosh installed on both reference hosts (brew on macOS, apt on Linux); a mosh session verified host to host over the tailnet via MagicDNS; UDP paths verified; a sleeping host woken by Wake-on-LAN.

Scope change: the phone terminal is now the PWA, so Blink Shell setup and phone mosh testing are dropped. mosh remains the raw-terminal fallback into hosts that run sshd. A host may keep Remote Login off entirely, by owner decision.

Gate: passed implicitly by the pivot decision; remaining phone verification moves to M2.

## M2: mushu-server MVP with web terminal (done)

Goal: from Safari on the iPhone, on any network, open the same live Herdr session as the desktop.

Steps:

1. Scaffold the Rust workspace: `server/` crate (axum + tokio + portable-pty) and `web/` front end (vanilla TS + xterm.js, embedded in the binary).
2. Terminal endpoint: WebSocket that spawns `herdr session attach` in a pty; client handles resize, reconnect with backoff, and touch keyboard basics (Ctrl, Esc, Tab, arrows toolbar).
3. Bind strictly to the host's Tailscale address; token auth on all endpoints.
4. Configure Tailscale Serve: port 443 where it is free, and a distinct port (for example 8443) on any host whose 443 already serves something else.
5. Run on each host as a user service (launchd on macOS, systemd on Linux).

Acceptance criteria:

- iPhone Safari opens `https://<host>.<tailnet>.ts.net[:port]`, attaches, and shows the same Herdr session as desktop Ghostty (action on one visible on the other).
- Works on home wifi and on 4G; a wifi-to-4G switch recovers the session within seconds via reconnect, with no lost Herdr state.
- Any service already published on a host remains reachable at its existing URL.
- Server unreachable from LAN and WAN addresses (tailnet bind verified).

Gate: passed 2026-07-30, validated from the phone on wifi, 4G, and 5G.

Post-gate correction (2026-07-31): the fixed client frame now follows the iOS visual viewport while the keyboard is open, refits the terminal and remote PTY, and keeps the active terminal prompt visible as the keyboard slides in. The owner validated the correction in the installed iPhone PWA; this does not change the passed M2 gate.

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

Multi-host UX shipped ahead of this milestone on 2026-07-31 (D9-D10): one installed app drives every host from a single origin, with per-host alert toggles and an optional Face ID lock on stored tokens. Inbox grouping and priorities are deferred; with a handful of agents the existing chips are not the constraint.

Steps:

1. Pairing and honest docs (done 2026-07-31): `mushuctl pair` prints a QR that signs a phone in without typing a token (D11), and the docs now match the shipped app, including the shared VAPID keypair that multi-host alerts require.
2. Release story (done 2026-07-31): prebuilt binaries for macOS and Linux on both architectures published by a tagged release, `install.sh` for one-line setup, and CI running fmt, clippy, and `cargo test` on every push. Installing no longer requires a Rust toolchain.
3. Quiet hours: a time window in the notifier loop, so a finished agent does not wake you at 3am.
4. Honest limitations doc versus commercial alternatives (image paste, Live Activities out of scope without a native app) and versus t3code.

Acceptance criteria:

- A third party following the docs alone can reach the M4 experience.
- Limitations and non-goals are documented.

Gate: user validates and decides what comes next.
