# Decisions

Decision records for the choices that shape mushu. D1 and D2 were superseded on 2026-07-30 after reviewing Blink Shell's App Store feedback and [t3code](https://github.com/pingdotgg/t3code) as prior art.

## D1: Blink Shell as the phone terminal (superseded by D6)

Original decision: target Blink Shell as the iOS terminal front end because it was the only open-source mosh client on iOS.

Superseded: App Store reviews are poor and the store version needs a subscription. Replaced by the mushu PWA (D6), which needs no terminal app at all.

## D2: Self-hosted ntfy as the push channel (superseded by D7)

Original decision: self-hosted ntfy behind Tailscale Serve, accepting the iOS APNs upstream relay caveat.

Superseded: with a PWA front end, iOS Web Push is strictly better (no extra app, no ntfy.sh relay, E2E encrypted payloads). ntfy remains a possible fallback channel if Web Push reliability disappoints in practice.

## D3: Rust for mushu-server

Decision: implement the host-side daemon in Rust (originally scoped as a notification bridge, now the full mushu-server).

Why: single static binary for macOS and Linux hosts, strong long-running daemon reliability, good WebSocket and pty ecosystem (axum, tokio, portable-pty), and it matches the project's spirit of building fully in the open what others ship closed.

Alternatives rejected: TypeScript + Bun (faster iteration but heavier runtime for a daemon), Go (fine, but Rust preferred by the owner).

## D4: mosh alongside SSH as fallback, not the primary phone path

Decision: SSH remains the default host-to-host transport; mosh (GPLv3) is the raw-terminal fallback into any host that runs sshd. The phone's primary path is the PWA over Tailscale.

Why: mosh solved roaming for a raw terminal, but the PWA gets equivalent resilience from aggressive WebSocket reconnect plus Herdr owning all session state. A host that keeps Remote Login (sshd) off has no mosh or SSH path at all, by design.

## D5: Herdr socket API as the source of truth for agent state

Decision: mushu-server derives agent state from Herdr's socket API (`herdr api snapshot | schema`, `herdr agent wait | list | get`) and uses Claude Code hooks / Codex notify only as low-latency triggers.

Why: Herdr already normalizes agent state across claude, codex, opencode, and cursor via its installed integrations, so mushu inherits multi-agent support instead of reimplementing per-agent detection from scratch.

## D6: mushu PWA as the phone front end

Decision: build a self-hosted web control surface, t3code-inspired: mushu-server (Rust) on each host serves a PWA with a web terminal (pty attach to Herdr over WebSocket), agent inbox, and approvals, reachable only via Tailscale Serve HTTPS on the tailnet.

Why: no dependency on any App Store terminal (Blink reviews are poor, alternatives are proprietary), no sshd needed on any host (macOS Remote Login only gates SSH, not a tailnet-bound web server), fully open source end to end, and one UI that can show all hosts.

Alternatives rejected: adopting t3code directly (MIT but early-stage, no Herdr integration, remote access may relay through their infrastructure), proprietary terminal apps (Termius, Secure ShellFish).

## D7: iOS Web Push for notifications

Decision: mushu-server sends Web Push notifications (VAPID) directly through Apple's push service to the installed PWA.

Why: no extra app, no third-party relay, payloads E2E encrypted per RFC 8291 so APNs transit reveals nothing, and notification taps deep-link straight into the PWA on the right host and session. Requires iOS 16.4+ and the PWA on the home screen, both acceptable.

Fallback: self-hosted ntfy (D2) can be revived as a secondary channel if iOS Web Push delivery proves unreliable.

## D8: macOS hosts included via mushu-server, Remote Login stays off

Decision: a macOS host runs mushu-server bound to its Tailscale address, making its Herdr sessions reachable from the phone without enabling Remote Login/sshd. Hosts that do run sshd keep SSH and mosh as additional fallback paths.

Why: a workstation is often where Ghostty + Herdr agents primarily run, while being the machine an owner is least willing to expose over sshd. A tailnet-bound web server satisfies both.

## D9: One installed app drives every host from a single origin

Decision: the PWA keeps running on the origin it was installed from and switches hosts by swapping the WebSocket and API base URL plus that host's token, instead of navigating to the other host's URL.

Why: each host is its own HTTPS origin, and navigating between origins inside an installed iOS web app opens the in-app browser overlay, whose WKWebView leaves the layout broken often enough that the header lands under the status bar and stops accepting taps. No in-page workaround fixed it reliably. Staying on one origin removes the navigation entirely, and it costs nothing: WebSockets are exempt from CORS, and the API routes already send permissive CORS headers with token auth unchanged.

Consequence: tokens for every host live in the installed app's storage (see D10), and a stale socket could reconnect to the previous host, so a connection epoch counter invalidates in-flight handlers on every switch.

## D10: Host tokens encrypted under a Secure Enclave passkey

Decision: offer an optional lock that encrypts all saved host tokens with AES-GCM, where the key comes from a WebAuthn passkey's PRF extension output, unlocked by Face ID or Touch ID.

Why: a PWA cannot put secrets in the iOS Keychain, so tokens otherwise sit in plain localStorage. A passkey's private key does live in the Secure Enclave, and PRF turns it into a stable per-credential secret that never leaves the device, giving real at-rest protection with no server involvement and no account system.

Alternatives rejected: server-side WebAuthn login replacing tokens (strongest, but needs registration, challenge, and session handling in mushu-server); a user-chosen passphrase (another secret to remember, and no biometric unlock).

## D11: Pairing by QR with the token in the URL fragment

Decision: `mushuctl pair` prints a QR code encoding `https://<host>/#<token>`; the app reads the fragment, stores the token, and strips it once running as an installed app.

Why: typing a 48-character token into a phone is the worst step of setup. A fragment is never transmitted to the server, so unlike a query string the token cannot reach an access log or proxy trace. Keeping it in the URL during the Safari visit matters too: iOS gives an installed web app a storage jar separate from Safari's, so a token merely written to storage before "Add to Home Screen" may not survive, while one carried in the bookmark URL always does.

Alternatives rejected: a short-lived one-time pairing code exchanged for the token (removes the token from the URL entirely, but needs a stateful endpoint, expiry and single-use handling, and a fresh QR whenever setup runs slow).
