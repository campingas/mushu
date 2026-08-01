# Development

How to build, run, and extend mushu. See [architecture.md](architecture.md) for what the pieces are and [decisions.md](decisions.md) for why they are that way.

## Layout

| Path | Contents |
|---|---|
| `server/src/main.rs` | routing, auth, WebSocket terminal, static assets, the `pair` subcommand |
| `server/src/agents.rs` | Herdr snapshot parsing, actions, and the push notifier loop |
| `server/src/push.rs` | VAPID keypair, subscription store, Web Push sending |
| `server/src/update.rs` | fixed-repository latest-stable checks, download validation, atomic replacement |
| `web/` | the PWA: `index.html`, `app.js`, `style.css`, `sw.js`, vendored xterm.js and jsQR |
| `scripts/mushuctl` | service control and phone pairing |
| `services/` | launchd and systemd unit templates |

## Local loop

Run a throwaway instance instead of touching your real service. A plain shell as `MUSHU_CMD` keeps Herdr out of the picture while working on the client:

```sh
cargo build --release
MUSHU_BIND=127.0.0.1:8498 MUSHU_HOST=alpha \
  MUSHU_TOKEN=devtokendevtoken MUSHU_CMD=/bin/sh \
  ./target/release/mushu-server
```

Open `http://127.0.0.1:8498/#devtokendevtoken`: the fragment signs you in, so no token prompt. Plain HTTP is fine locally, but service workers and Web Push need HTTPS, so notifications can only be exercised through Tailscale Serve.

To work on multi-host behaviour, start a second instance on another HTTPS origin with a different `MUSHU_HOST` and shared VAPID key, then pair its `mushuctl pair` QR from Settings. Host switching, per-host alert toggles, and the drawer all work against two instances. The in-app parser intentionally rejects HTTP pairing URLs, so local QR pairing needs a trusted local HTTPS endpoint; use direct storage setup only for isolated browser-test fixtures, not as a product flow.

**The web assets are embedded into the binary** at compile time by `rust-embed` (`#[derive(RustEmbed)] #[folder = "../web"]`). Editing anything in `web/` therefore requires rebuilding the binary, and cargo does not always notice a change confined to `web/`; touch a source file or `cargo clean -p mushu-server` if a change seems not to apply. Assets are served with `Cache-Control: no-cache` because without it browsers kept serving the pre-upgrade bundle after a deploy.

`web/manifest.webmanifest` deliberately has **no `start_url`**. Do not add one back: iOS launches an installed web app at `start_url` when it is present, which discards the `#token` fragment that `mushuctl pair` relies on, and the freshly installed app then prompts for a token. With the key absent, `start_url` defaults to the document URL that was added to the home screen, so the fragment survives the install.

## Extending

Adding an endpoint: register it on the router in `main.rs`, and gate anything sensitive with the existing `authed(&headers, &state)` helper, which compares the `x-mushu-token` header in constant time. The `CorsLayer` already allows that header cross-origin, which is what lets one installed app drive every host (D9); a new endpoint the client calls on *other* instances needs nothing extra, but one that must never be called cross-origin should be excluded from the layer.

Agent state comes from Herdr and nothing else (D5). New agent-facing behaviour usually means a new action in `agents.rs` mapped onto a `herdr` subcommand, not new state tracking here. Actions carry `state_change_seq` and are rejected with 409 when stale, so any new action should keep that guard.

Notifications are sent from the notifier loop in `agents.rs`, which polls every two seconds. The first snapshot seeds state without replaying existing events. A pane must then be blocked in two consecutive snapshots before one attention notification is sent; that incident stays latched across sequence changes and transient oscillation until two consecutive nonblocked snapshots or pane removal. Working-to-done/idle transitions still send completion notifications. Send through the loop rather than calling the push store from a new place, so suppression logic stays in one spot.

`GET /api/attention?pane_id=...` is token-authenticated and only returns context while the current Herdr snapshot still reports that pane as blocked with the same sequence before and after the read. It reads `herdr agent read <pane> --source detection --lines 40 --format text`, caps the returned tail at 12 KiB, and recognizes choices only as a contiguous 2-9 item numbered block starting at 1 near the bottom. Keep terminal context out of push payloads; pushes contain only generic display text and the non-secret instance URL, pane ID, and observed sequence needed to route this fetch after vault unlock.

Client state lives in `localStorage`: `mushu_instances` (host URLs and tokens), `mushu_active`, and `mushu_vault` when the Face ID lock is on. Anything added there must go through `saveInstances()`, which writes to the encrypted vault when it is enabled.

Settings is a full safe-area page. Because it is an absolute child of the padded fixed body, its header must include `--safe-top` itself so its title and close control stay below iOS system chrome. Additional hosts are QR-only: jsQR 1.4.0 is vendored under Apache-2.0 in `web/vendor/`, camera capture asks for the environment-facing camera, image import is decoded locally, and a candidate is saved only after authenticated `/api/host` validation and exact VAPID-key comparison. Stop media tracks, detach `srcObject`, zero the canvas, clear the file input, and release candidate/token references on success, failure, cancel, and Settings close.

Notification routing also uses `mushu_pending_attention` for non-secret instance URL, pane ID, and sequence metadata. Register the service-worker listener before waiting for Face ID, keep this value and any cold-launch query until the target is consumed after unlock, then clear both; never store a token or terminal context there.

Before every terminal WebSocket connection, the client applies its static fallback and makes an authenticated, two-second-bounded `/api/host` preflight against the captured active instance. Keep that fetch ahead of socket construction, abort it on a host switch, and guard its result with `connectEpoch`; theme discovery must always continue to the terminal on success, timeout, malformed configuration, or an older server without the endpoint.

For a `MUSHU_CMD` whose executable basename is `herdr`, `/api/host` reads only `[theme]` and `[theme.custom]` from the inherited Herdr config path (`HERDR_CONFIG_PATH`, then `$XDG_CONFIG_HOME/herdr/config.toml`, then `$HOME/.config/herdr/config.toml`). The response is a normalized descriptor, never raw configuration, filesystem paths, commands, or parser diagnostics. Missing or unusable configuration uses Herdr's documented `catppuccin` default; non-Herdr commands return a null theme.

## Mobile terminal controls

The terminal toolbar is intentionally compact enough for a 320px viewport: Esc, Tab, Ctrl, a disabled move placeholder, an image shortcut, and a microphone shortcut. Both image and microphone shortcuts open the same Compose panel; the image shortcut does not choose an agent or open the gallery, so the named-agent-only Screenshot control remains the only attachment entry point. A deliberate terminal tap toggles xterm keyboard focus; pointer movement does not toggle it. When Herdr has mouse tracking active, touch drags are translated into xterm wheel events so Herdr can scroll its pane history. Without mouse tracking, xterm keeps its native touch scrollback path. Toolbar quick keys restore the keyboard state that existed before the tap.

The keyboard-dismiss path depends on iOS event order: when a touch `pointerdown` starts while xterm's textarea is focused, set `suppressTerminalMouseDown`, then intercept the touch-generated compatibility `mousedown` on `#term` in the capture phase with `preventDefault()` and `stopPropagation()` before xterm can prevent the event and refocus itself; an unmoved `pointerup` can then blur the terminal. Delaying the blur, including with `requestAnimationFrame`, is not sufficient: owner testing showed the keyboard begin to slide down and then reopen. Keep the suppression touch-only and focused-only so desktop mouse input is unchanged, a drag remains scrolling without dismissal, and a tap with the keyboard closed still focuses the terminal.

The rounded toolbar surface deliberately has no safe-area padding and sits 2px from the visual viewport bottom when the keyboard is closed, then immediately above the native iOS assistant area when it is open. The visual viewport already excludes the covered region, so adding safe-area padding to `#toolbar` creates an unwanted gap.

Compose accepts multiline typed, pasted, or dictated text. Terminal sends go through xterm's paste path so multiline and terminal paste modes are preserved; Send + Enter adds a carriage return after the pasted text. Agent-target sends still use the prompt action unchanged. The Paste button uses the Clipboard API and falls back to instructions for pasting directly into the field when clipboard access is unavailable or denied.

Compose also accepts one screenshot from the phone gallery when a named agent is selected. The client captures the host URL, token, pane ID, and sequence before sending, normalizes the selection through a bounded canvas to PNG, and posts `FormData` without setting `Content-Type`; terminal targets retain their existing text-only paste path. Image preparation disables sending and target changes and uses a generation guard so a host switch, close, or later selection cannot install a stale result. A host switch resets and closes Compose rather than combining an old agent with a new active host. Keep text and the preview after definite validation or stale failures and clear them only after a 204 response; a network failure after dispatch has unknown delivery status, so disable resending until Compose is closed and tell the owner to check the agent first.

`POST /api/prompt-image` authenticates before parsing multipart data and accepts exactly one `pane_id`, `seq`, optional `text`, and PNG `image`. Its 10 MiB body limit is route-local, only one upload is parsed and normalized at a time, decoder allocation is capped, dimensions are limited to 4096 per side and 12 million pixels, and the server decodes and re-encodes the PNG before writing a generated 0600 file beneath `$XDG_CACHE_HOME/mushu/uploads` or `$HOME/.cache/mushu/uploads` in a 0700 directory. Files expire after 24 hours and the startup, upload, and 15-minute periodic cleanup paths only touch expired regular files with the exact generated name shape, skipping symlinks and unrelated files. Any stale target or Herdr failure after storage removes the new file and logs a cleanup failure rather than silently ignoring it.

Screenshot audit entries contain the generated ID/path, normalized byte size and dimensions, pane and named agent, and result; they never contain image bytes. Native share-sheet input, clipboard images, multiple images, terminal attachments, remote storage, and longer-lived media management remain outside this endpoint.

A PWA cannot remove or customize Safari's native iOS keyboard accessory bar. Terminal focus, scrolling, dictation, gallery decoding, screenshot preview and upload, clipboard permission behavior, and the installed-PWA keyboard layout must be validated by the owner on an iPhone; desktop browser checks only verify the client logic.

## TLS constraint

`web-push` reaches OpenSSL through its `ece` dependency on **every** platform, not only Linux, and offers no rustls option. Default builds therefore link the system OpenSSL, which on macOS means an absolute Homebrew path that does not exist on other machines. Released binaries are built with `--features vendored-tls`, which compiles OpenSSL from source and links it statically. Local development does not need the feature; anything producing a binary for someone else does.

## Releasing

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on every branch push. Keep those green locally before pushing.

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which first requires the tag to equal `v` plus `server/Cargo.toml`'s package version, then builds four targets (macOS and Linux, x86_64 and aarch64), publishes them with `mushuctl` and a `SHA256SUMS` file, and generates release notes. Tagged binaries embed the tag, `GITHUB_SHA`, and `stable` kind; `mushu-server --version` exposes all three. Both macOS binaries are built on the arm64 runner, the x86_64 one cross-compiled, because the Intel runner has been retired. The workflow also accepts `workflow_dispatch`, which produces `dev` binaries without publishing anything.

`GET /api/update` is authenticated and accepts only the optional `?refresh=true` cache bypass. A successful explicit refresh also clears a prior failed install state so the owner can retry after a transient failure; a failed release check preserves the install failure. `POST /api/update` accepts only `{ "tag": "v…" }`, re-fetches the fixed latest stable release, and rejects development builds, non-newer/stale tags, unknown fields, and another running job. The updater maps the running OS/architecture to one exact release asset, validates its fixed GitHub download URL, bounded size, exact `SHA256SUMS` entry, and staged stable `--version`, then fsyncs and atomically replaces `current_exe` while retaining `<binary>.previous`. Do not add client-supplied repositories, URLs, or a second restart mechanism.

Settings update checks report progress globally and on every host row. Keep one global check in flight, leave failed hosts individually retryable, and bind asynchronous row updates to stable host identities so a host removal, reorder, or rerender cannot receive another host's result.

Exercise refusal paths with a normal development build: authenticated `GET /api/update` should report `kind: dev` and `install_allowed: false`, unauthenticated access should return 401, and an authenticated POST should return 409 without contacting the release installer. Full replacement is validated only with a disposable tagged stable binary and fake release fixture; never point a development check at the live installed service.

`install.sh` resolves `releases/latest`, so it only works once a release exists.
