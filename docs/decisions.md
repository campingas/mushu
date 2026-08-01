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

## D12: Adapt Herdr themes per host at terminal connection time

Decision: before each terminal WebSocket connection, including host switches and reconnects, the client fetches an authenticated theme descriptor from that host and applies an adapted Mushu palette to both the PWA and xterm. Herdr's `terminal` theme follows the phone's dark or light color scheme so both phone surfaces stay coherent; it does not attempt parity with desktop Ghostty or another terminal emulator.

Why: each host can intentionally use a different Herdr theme, but copying version-specific upstream constants would make Mushu brittle and still could not reproduce a desktop terminal's rendering. A small normalized descriptor preserves the theme identity and custom accents while Mushu retains readable phone-specific surfaces and contrast guards.

Privacy and platform limits: the server reads only Herdr's theme table from its inherited config location, caps the read, and returns only normalized theme names and supported color tokens. It never returns raw TOML, paths, commands, or parser diagnostics. The web manifest keeps a static fallback color because installed PWA metadata cannot vary per connected host; only the runtime `theme-color` meta tag follows the active palette.

## D13: Attention notifications carry routing, then fetch context after unlock

Decision: notify only after two consecutive blocked snapshots, latch that pane until two consecutive nonblocked snapshots or pane removal, and put only the saved instance URL, pane ID, and observed sequence beside the generic title, body, and host in the encrypted push payload. A notification tap keeps the installed app on its own origin, switches to the exact saved instance in place, and fetches the current prompt through authenticated `GET /api/attention` after the Face ID vault is unlocked.

Why: `state_change_seq` can change while an agent remains blocked, and brief status oscillation can otherwise produce duplicate notifications for one incident. Routing metadata is enough to find the live request without placing terminal text in a payload delivered while the phone is locked. The post-unlock endpoint confirms that the pane remains blocked at the same sequence across its bounded read; a changed request returns conflict instead of pairing old context with a new actionable sequence.

Choice safety: the action card treats a prompt as multiple choice only when a contiguous block of 2-9 numbered options starts at 1 near the bottom of the detection text. Anything ambiguous gets only Open terminal, Deny/Esc, and Approve/Enter; a detected choice prompt gets its explicit numbered options plus Open terminal and Deny/Esc, with no generic approval button.

Alternatives rejected: terminal context in Web Push (unnecessary lock-screen disclosure), direct notification action buttons (not consistently available for installed iOS PWAs), navigating to another host origin (breaks the single-origin installed-app invariant in D9), and trusting the sequence from the notification without refreshing current state (racy and replay-prone).

## D14: Hosts update only to the revalidated latest stable release

Decision: a tagged stable Mushu binary may update itself only to a newer plain-semver release returned by GitHub's fixed `campingas/mushu` latest-release endpoint. Settings can refresh the check and request that exact tag, but the daemon independently repeats the latest-stable check before starting and never accepts a repository, URL, asset, or version chosen by the client.

Why: a phone-friendly self-update path is useful only if it does not turn an authenticated PWA bug into an arbitrary executable downloader. Release binaries embed tag, commit SHA, and `stable` kind; local and workflow-dispatch builds are `dev` and refuse installation. One atomic job downloads the exact platform asset and checksum manifest with bounds, verifies both checksum and staged `--version`, fsyncs, preserves `.previous`, replaces the executable by same-directory rename, then uses the existing shutdown watch and re-exec path so WebSockets reconnect normally.

Confirmation: installs never start from a background check. Every update needs an explicit themed host/current/latest confirmation, and a vault-enabled app obtains a fresh platform passkey PRF result before showing it.

Alternatives rejected: prerelease or historical version selection (wider downgrade and compatibility surface), arbitrary release URLs or forks (turns the daemon into a remote code installer), automatic background installation (surprising host mutation), signed attestations beyond published SHA-256 checksums (future hardening, not required for this release), and automatic runtime rollback (service-manager and health-policy work outside this update boundary).

## D15: Screenshot prompts use bounded private host files

Decision: Compose may attach exactly one gallery image only when targeting a named agent. The browser decodes and scales the selection, renders it to PNG, and sends multipart form data to the captured host, pane, and sequence; the authenticated server independently bounds and decodes it, re-encodes a metadata-free PNG, stores it under its private XDG or home cache upload directory, and gives Herdr one controlled prompt containing that generated local path.

Why: coding agents already understand local image paths, so a short-lived host file uses their existing input surface without teaching Mushu or Herdr a new binary protocol. Server-side decode and re-encode makes client normalization a usability optimization rather than a trust boundary, generated names avoid filename injection, and the existing `state_change_seq` check prevents a delayed upload from reaching a different agent state.

Retention and audit: successful files expire after 24 hours so an agent can read them after dispatch. Startup, upload, and 15-minute periodic cleanup remove only expired matching regular upload files, never symlinks or unrelated cache contents; failed post-storage validation or Herdr dispatch removes the new file immediately and reports removal failures. Audit records identify the generated upload, normalized byte size and dimensions, named target, and result, but never image bytes.

Resource and delivery safety: the larger multipart body limit applies only to this route, one authenticated upload is parsed and decoded at a time, and the PNG decoder has explicit dimension and allocation limits. Compose is reset on a host switch and image preparation is generation-guarded. Because a lost HTTP response can make delivery ambiguous without proving Herdr failed, the client blocks an immediate retry and tells the owner to check the agent before closing and trying again.

Alternatives rejected: embedding image bytes in a prompt (large and unsupported), keeping original gallery files or names (metadata and injection risk), remote object storage (unnecessary privacy and operations surface), terminal attachments, multiple images, clipboard-image integration, and a native iOS share extension.
