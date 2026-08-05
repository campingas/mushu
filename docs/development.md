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
| `scripts/mushuctl` | service unit rendering, service control, and phone pairing |

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

The terminal toolbar is intentionally compact enough for a 320px viewport: Esc, Tab, Ctrl, an arrow pad toggle, an image shortcut, and a microphone shortcut. Both image and microphone shortcuts open the same Compose panel; the image shortcut does not choose an agent or open the gallery, so the named-agent-only Screenshot control remains the only attachment entry point. A deliberate terminal tap toggles xterm keyboard focus; pointer movement does not toggle it. When Herdr has mouse tracking active, touch drags are translated into xterm wheel events so Herdr can scroll its pane history. Without mouse tracking, xterm keeps its native touch scrollback path. Toolbar quick keys restore the keyboard state that existed before the tap.

The move icon opens an arrow pad floating above the toolbar: delete, up, paste on the top row, left, enter, right on the middle row, and down centred below. It is a sibling of `#toolbar` rather than a child because `#toolbar` is `overflow: hidden` and lays its buttons out as flex siblings; JS sets `bottom` from the live toolbar height when the pad opens and on every visual-viewport change, so the pad tracks the toolbar as the iOS keyboard slides in. Pad taps share the toolbar's `pointerdown`/`click` pair through `rememberKeyboard()` and `restoreKeyboard()`: without that, every tap would blur xterm and dismiss the keyboard, making the pad usable exactly once. Any pointer outside the pad closes it, including the other toolbar buttons, so it never covers the terminal while something else has focus.

Cursor keys honour DECCKM. `cursorKey()` emits `ESC [ A` normally and `ESC O A` while the application cursor keys mode is set, read from `term.modes.applicationCursorKeysMode`, so a full-screen program gets the form it asked for. The pad's Paste reuses the Clipboard API path and falls back to a press-and-hold instruction; on iOS a clipboard read also raises Safari's own paste confirmation, so pasting is two taps rather than one.

Each host can be renamed from Settings by tapping its name. The nickname is stored as an optional `name` on the `mushu_instances` entry, so it goes through `saveInstances()` and inherits the vault encryption when the Face ID lock is on; it is never sent to the host. `displayName()` resolves a label as nickname, then the name `/api/host` or `/api/agents` last reported this session, then the URL label, so one rename reaches the header chip, the drawer, and the Settings card at once. The chip colour follows it, because `hostHue()` hashes whatever name is shown. An empty answer clears the nickname and discovery takes the name back over.

Both discovery paths record the reported name but never override a nickname: theme discovery in `connect()` and the four-second agent poll in `refreshAgents()` both write through the same guard. Missing the guard in the poll is what made an earlier version silently revert a rename a few seconds later. Discovered names are cached in memory rather than in `mushu_instances`, so a stale label can never persist to disk. The rename control is the host name itself rather than a fourth button in `.host-controls`, which starved the copy column until the host URL broke mid-word on a 390px card.

A nickname is per device and never reaches the host, so push notification bodies still carry the host's own name from `MUSHU_HOST`. A service worker cannot resolve one: it has no `localStorage` access, and with the vault on the instance list is encrypted until a passkey unlock. Set `MUSHU_HOST` on the host itself when the name must match everywhere.

The keyboard-dismiss path depends on iOS event order: when a touch `pointerdown` starts while xterm's textarea is focused, set `suppressTerminalMouseDown`, then intercept the touch-generated compatibility `mousedown` on `#term` in the capture phase with `preventDefault()` and `stopPropagation()` before xterm can prevent the event and refocus itself; an unmoved `pointerup` can then blur the terminal. Delaying the blur, including with `requestAnimationFrame`, is not sufficient: owner testing showed the keyboard begin to slide down and then reopen. Keep the suppression touch-only and focused-only so desktop mouse input is unchanged, a drag remains scrolling without dismissal, and a tap with the keyboard closed still focuses the terminal.

The rounded toolbar surface deliberately has no safe-area padding and sits 2px from the visual viewport bottom when the keyboard is closed, then immediately above the native iOS assistant area when it is open. The visual viewport already excludes the covered region, so adding safe-area padding to `#toolbar` creates an unwanted gap.

Compose accepts multiline typed, pasted, or dictated text. Terminal sends go through xterm's paste path so multiline and terminal paste modes are preserved; Send + Enter adds a carriage return after the pasted text. Agent-target sends still use the prompt action unchanged. The Paste button uses the Clipboard API and falls back to instructions for pasting directly into the field when clipboard access is unavailable or denied.

Compose also accepts one screenshot from the phone gallery when a named agent is selected. The client captures the host URL, token, pane ID, and sequence before sending, normalizes the selection through a bounded canvas to PNG, and posts `FormData` without setting `Content-Type`; terminal targets retain their existing text-only paste path. Image preparation disables sending and target changes and uses a generation guard so a host switch, close, or later selection cannot install a stale result. A host switch resets and closes Compose rather than combining an old agent with a new active host. Keep text and the preview after definite validation or stale failures and clear them only after a 204 response; a network failure after dispatch has unknown delivery status, so disable resending until Compose is closed and tell the owner to check the agent first.

`POST /api/prompt-image` authenticates before parsing multipart data and accepts exactly one `pane_id`, `seq`, optional `text`, and PNG `image`. Its 10 MiB body limit is route-local, only one upload is parsed and normalized at a time, decoder allocation is capped, dimensions are limited to 4096 per side and 12 million pixels, and the server decodes and re-encodes the PNG before writing a generated 0600 file beneath `$XDG_CACHE_HOME/mushu/uploads` or `$HOME/.cache/mushu/uploads` in a 0700 directory. Files expire after 24 hours and the startup, upload, and 15-minute periodic cleanup paths only touch expired regular files with the exact generated name shape, skipping symlinks and unrelated files. Any stale target or Herdr failure after storage removes the new file and logs a cleanup failure rather than silently ignoring it.

Screenshot audit entries contain the generated ID/path, normalized byte size and dimensions, pane and named agent, and result; they never contain image bytes. Native share-sheet input, clipboard images, multiple images, terminal attachments, remote storage, and longer-lived media management remain outside this endpoint.

A PWA cannot remove or customize Safari's native iOS keyboard accessory bar. Terminal focus, scrolling, dictation, gallery decoding, screenshot preview and upload, clipboard permission behavior, and the installed-PWA keyboard layout must be validated by the owner on an iPhone; desktop browser checks only verify the client logic.

## TLS constraint

`web-push` reaches OpenSSL through its `ece` dependency on **every** platform, not only Linux, and offers no rustls option. Default builds therefore link the system OpenSSL, which on macOS means an absolute Homebrew path that does not exist on other machines. Released binaries are built with `--features vendored-tls`, which compiles OpenSSL from source and links it statically. Local development does not need the feature; anything producing a binary for someone else does.

## Settings and host detail

Settings is two views inside one overlay. The list answers only which hosts exist, whether each is reachable, and whether alerts are on: an OS mark, the name, a bell, a chevron. Everything editable lives on the host detail page behind `openHostDetail()`, which hides `#settings-scroll` and shows `#host-detail`: rename, published address, origin, system, build identity, the alerts toggle, the update check and install, and removal. Both views delegate to one `onHostControlClick` handler bound to each container, so the rename, remove, alerts and update paths have a single implementation regardless of which view raised them.

`loadUpdate()` writes through `[data-update-status]` and `[data-update-key]`, which now exist only on the detail page, so `renderHostDetail()` calls it after rendering. The list no longer carries those attributes; `loadUpdate` already tolerates their absence, which is what keeps the global "check updates" control working from the list.

Reachability and identity are tracked separately, in `hostUp` and `hostInfo`. A host that stops answering keeps its last descriptor, so its OS mark greys out instead of reverting to a generic glyph; discarding the descriptor on failure would throw away the identity the list exists to show. Every host is probed with `/api/host` when Settings opens, which is also where non-active hosts pick up their real names.

No host is given a different border. The active host is a selection, not a status, and colouring it read as an alert.

## Icons

UI icons come from Lineicons (MIT) in `web/vendor/icons`, and so do the OS marks in `web/vendor/brands`. The two agent marks come from Bootstrap Icons (MIT) and the generic Linux mark from Simple Icons (CC0), because Lineicons carries neither. Only the icons actually used are vendored, each with a license file beside it. Lineicons ship a hardcoded hex and fixed pixel dimensions, so each file is normalised to `fill="currentColor"` with the width and height removed; without that the stylesheet cannot tint or size them.

Simple Icons has removed its OpenAI and Microsoft marks, and Lineicons has no OpenAI, Claude or generic Linux mark, so no single set covers everything; each mark is taken from the set that still publishes it.

Linux hosts resolve to their distribution where a mark exists, so an Ubuntu machine shows the Ubuntu circle of friends in `#e95420` rather than a generic Tux, and the System row reads `ubuntu 26.04` because `os_version` carries `VERSION_ID` from the same `os-release` read. macOS and Windows report no version, so their row is just the system name. The mapping lives in `osOf` on the client and `linux_flavour` on the server; adding a distribution means one entry in each plus the vendored mark. A host running a Mushu older than this change sends no `os` at all and falls back to the generic glyph.

Agent and OS marks are drawn in their official colour, held in `--brand-*` custom properties. Apple and OpenAI publish black marks whose dark-background variant is white, so those two are white here. State is expressed on the chip border only: recolouring the mark by status meant an agent was never shown in its own colour.

Icon loading is awaited through `iconsReady` before any render that draws one, including `renderHeader()`. This is not cosmetic. The gallery compares pixels exactly, and drawing chips before the marks resolve produced a baseline that passed when written and failed when re-checked.

## Visual regression gallery

The Playwright suite serves the real files in `web/` and supplies deterministic API, terminal socket, local-storage, and service-worker fixtures only inside the browser test. There is no production demo mode. Its six 390x844 dark Chromium baselines live in `output/playwright/mushu-gallery`: a Claude session in the terminal, the two-host drawer, screenshot Compose, the Settings host list, a host detail page, and the real in-app notification attention card.

Install the pinned dependency with `bun install --frozen-lockfile`, compare the current UI with `bun run visual:check`, and intentionally replace all committed baselines with `bun run visual:update`. Both commands route through `scripts/visual`, which uses the same pinned Playwright Noble container as CI so macOS and Linux do not produce competing baselines. Never update baselines as part of the check command or in CI: inspect every changed PNG before accepting it.

The terminal fixture pins its font by wrapping the `Terminal` constructor in an init script, and the transcript is laid out against the 48x50 grid that pin produces. The container's generic `monospace` resolves to WenQuanYi Zen Hei Mono, a CJK face that carries no quadrant block glyphs, so the Claude header art fell back to a proportional font and landed off the cell grid. Liberation Mono and FreeMono both advance 0.6em, so a glyph FreeMono supplies still lands on the cell FreeMono did not draw. The override has to reach the constructor because xterm measures its cell from the `fontFamily` option and ignores later CSS. If you change the transcript, keep every line under 48 columns and the total line count at 50: a wider line wraps and a longer transcript pushes the composer off the bottom row.

All visual fixtures must stay anonymous. Use reserved `.test` hostnames, generic project paths and messages, and fixture-only tokens; never copy a real host, tailnet, user path, token, terminal transcript, or deployment detail into the tests or screenshots. The suite asserts stable view state, 390px horizontal fit, fixture anonymity, and unexpected console or request failures. CI uses the Playwright 1.62.0 Noble image matching `@playwright/test` 1.62.0, fails on any pixel difference, and uploads failure screenshots, diffs, and traces.

These checks cover deterministic Chromium rendering and client logic only. They do not validate an installed iOS PWA, native keyboard or safe-area behavior, camera/gallery interaction on a phone, service-worker delivery, or Web Push notification delivery; those remain explicit owner-validation boundaries.

## Releasing

`tests/mushuctl.sh` exercises service rendering, lifecycle failure, secret validation, VAPID replacement, uninstall scope, and installer guidance in temporary homes with fake launchd and systemd commands. It never calls the live service manager. Run it after changing `scripts/mushuctl` or `install.sh`; `plutil` lints the emulated macOS render when available.

CI (`.github/workflows/ci.yml`) runs that isolated shell suite, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on every branch push. Keep those green locally before pushing.

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which first requires the tag to equal `v` plus `server/Cargo.toml`'s package version, then builds four targets (macOS and Linux, x86_64 and aarch64), publishes them with `mushuctl` and a `SHA256SUMS` file, and generates release notes. Tagged binaries embed the tag, `GITHUB_SHA`, and `stable` kind; `mushu-server --version` exposes all three. Both macOS binaries are built on the arm64 runner, the x86_64 one cross-compiled, because the Intel runner has been retired. The workflow also accepts `workflow_dispatch`, which produces `dev` binaries without publishing anything.

`GET /api/update` is authenticated and accepts only the optional `?refresh=true` cache bypass. A successful explicit refresh also clears a prior failed install state so the owner can retry after a transient failure; a failed release check preserves the install failure. `POST /api/update` accepts only `{ "tag": "v…" }`, re-fetches the fixed latest stable release, and rejects development builds, non-newer/stale tags, unknown fields, and another running job. The updater maps the running OS/architecture to one exact release asset, validates its fixed GitHub download URL, bounded size, exact `SHA256SUMS` entry, and staged stable `--version`, then fsyncs and atomically replaces `current_exe` while retaining `<binary>.previous`. Do not add client-supplied repositories, URLs, or a second restart mechanism.

Settings update checks report progress globally and on every host row. Keep one global check in flight, leave failed hosts individually retryable, and bind asynchronous row updates to stable host identities so a host removal, reorder, or rerender cannot receive another host's result.

Exercise refusal paths with a normal development build: authenticated `GET /api/update` should report `kind: dev` and `install_allowed: false`, unauthenticated access should return 401, and an authenticated POST should return 409 without contacting the release installer. Full replacement is validated only with a disposable tagged stable binary and fake release fixture; never point a development check at the live installed service.

`install.sh` resolves `releases/latest`, so it only works once a release exists.
