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

Why: single static binary for macOS and Linux hosts, strong long-running daemon reliability, good WebSocket and pty ecosystem (axum, tokio, portable-pty), and it matches the project's spirit of doing properly in the open what Moshi did as a closed rewrite.

Alternatives rejected: TypeScript + Bun (faster iteration but heavier runtime for a daemon), Go (fine, but Rust preferred by the owner).

## D4: mosh alongside SSH as fallback, not the primary phone path

Decision: SSH remains the default host-to-host transport; mosh (GPLv3, installed and verified on both hosts) is the raw-terminal fallback into robrog. The phone's primary path is the PWA over Tailscale.

Why: mosh solved roaming for a raw terminal, but the PWA gets equivalent resilience from aggressive WebSocket reconnect plus Herdr owning all session state. The Mac keeps Remote Login (sshd) off entirely, so mosh/SSH to the Mac is intentionally impossible.

## D5: Herdr socket API as the source of truth for agent state

Decision: mushu-server derives agent state from Herdr's socket API (`herdr api snapshot | schema`, `herdr agent wait | list | get`) and uses Claude Code hooks / Codex notify only as low-latency triggers.

Why: Herdr already normalizes agent state across claude, codex, opencode, and cursor via its installed integrations, so mushu inherits multi-agent support instead of reimplementing per-agent detection the way moshi-hook does.

## D6: mushu PWA as the phone front end

Decision: build a self-hosted web control surface, t3code-inspired: mushu-server (Rust) on each host serves a PWA with a web terminal (pty attach to Herdr over WebSocket), agent inbox, and approvals, reachable only via Tailscale Serve HTTPS on the tailnet.

Why: no dependency on any App Store terminal (Blink reviews are poor, alternatives are proprietary), no sshd needed on any host (macOS Remote Login only gates SSH, not a tailnet-bound web server), fully open source end to end, and one UI that can show all hosts.

Alternatives rejected: adopting t3code directly (MIT but early-stage, no Herdr integration, remote access may relay through their infrastructure), proprietary terminal apps (Termius, Secure ShellFish).

## D7: iOS Web Push for notifications

Decision: mushu-server sends Web Push notifications (VAPID) directly through Apple's push service to the installed PWA.

Why: no extra app, no third-party relay, payloads E2E encrypted per RFC 8291 so APNs transit reveals nothing, and notification taps deep-link straight into the PWA on the right host and session. Requires iOS 16.4+ and the PWA on the home screen, both acceptable.

Fallback: self-hosted ntfy (D2) can be revived as a secondary channel if iOS Web Push delivery proves unreliable.

## D8: Mac included via mushu-server, Remote Login stays off

Decision: the MacBook runs mushu-server bound to its Tailscale address, making its Herdr sessions reachable from the phone without enabling Remote Login/sshd. robrog additionally keeps SSH and mosh as fallback paths.

Why: the Mac is where Ghostty + Herdr agents primarily run, and the owner explicitly refuses sshd exposure on it. A tailnet-bound web server satisfies both.
