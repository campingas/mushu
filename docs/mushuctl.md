# mushuctl

`mushuctl` controls the Mushu user service on macOS launchd and Linux systemd without printing the service environment or token.

## Install

Build and install the server and controller:

```sh
cargo build --release
install -d "$HOME/.local/bin"
install -m 755 target/release/mushu-server "$HOME/.local/bin/mushu-server"
install -m 755 scripts/mushuctl "$HOME/.local/bin/mushuctl"
```

`mushuctl install-service` creates the token below, so there is no separate step for it. The token file must be a regular file, contain at least 16 characters, and have no group or other permissions on Unix. `mushu-server` reads `MUSHU_TOKEN_FILE` directly, trims surrounding whitespace, and gives it precedence over the compatible `MUSHU_TOKEN` environment variable.

### Service unit

One command renders and installs the unit for the current platform, then starts it (D16):

```sh
mushuctl install-service
```

On macOS it writes `$HOME/Library/LaunchAgents/dev.mushu.server.plist`, lints it with `plutil` when available, and bootstraps it into the user's GUI domain. On Linux it writes `$HOME/.config/systemd/user/mushu.service`, reloads the manager, and enables and starts the unit. Both renders prefer the canonical `mushu-server` beside `mushuctl`, then fall back to a verified executable from PATH. They resolve absolute paths, safely quote platform values including spaces, and put the installed `herdr` directory on the service PATH, because neither manager inherits a login shell environment.

If `MUSHU_TOKEN_FILE`, or `$HOME/.config/mushu-token` by default, does not exist, `install-service` generates 24 random bytes as hex and writes them under `umask 077` with `noclobber`, so the file is never briefly readable by anyone else and a concurrent writer loses safely. It uses `openssl` when present and `/dev/urandom` otherwise, and it refuses to continue if it cannot obtain 24 bytes rather than installing a weak token. Both managers restart on failure, so a unit installed without a token would crash-loop instead of reporting the problem.

An existing token is never replaced, including under `--force`, since a new token would lock out every phone already paired to this host. Rotating a token is therefore a deliberate manual act: remove the file, re-run `install-service`, and pair each phone again.

`mushuctl install-service --print` writes the rendered unit to stdout and installs nothing, which is the way to review it before it lands or to adapt it by hand.

An existing unit that differs from the render is never replaced without `--force`. Mushu is configured through environment variables, and for a service those live in the unit, so a unit edited to set `MUSHU_HOST`, `MUSHU_BIND`, or `MUSHU_CMD` is configuration that a silent overwrite would discard. `--force` atomically copies the previous file to `<unit>.bak` before replacement. Every new unit is rendered mode `0600` into a same-directory temporary file, validated, and atomically renamed. Re-running on a host that is already correct rewrites nothing and does not restart.

Enable linger with `loginctl enable-linger "$USER"` only if Mushu must start before login and remain available after logout; this system-level policy may require administrator approval.

## Commands

- `mushuctl install-service` renders the launchd plist or systemd user unit, installs it, and starts Mushu. `--print` writes the unit to stdout instead; `--force` replaces an existing unit after backing it up to `<unit>.bak`.
- `mushuctl start` starts Mushu.
- `mushuctl stop` stops Mushu and its open web terminal sessions; it does not stop the persistent Herdr server.
- `mushuctl restart` restarts Mushu.
- `mushuctl status` reports only sanitized active state, plus launchd loaded state on macOS.
- `mushuctl logs` follows the launchd log file or systemd journal.
- `mushuctl pair` finishes by offering to copy another host's VAPID key to this one, since a phone that already receives notifications from a different host cannot receive them from this one until both share a keypair. It asks which host, shows the exact command, and runs it only on `y` or `yes`. It pulls rather than pushes: this host is the new one and has no subscriptions to lose. The offer appears only when stdin and stdout are both terminals, so scripted use is unchanged, and a blank host name skips it. Override the assumed remote path with `MUSHU_REMOTE_MUSHUCTL`.
- `mushuctl pair` prints a QR code that signs a phone in to this host. It defaults `MUSHU_TOKEN_FILE` to `$HOME/.config/mushu-token` and resolves the public URL from the Tailscale Serve mapping that proxies this host's bind address; set `MUSHU_URL` when there is no such mapping. The QR carries the token in the URL fragment, and the URL and token are printed underneath as a fallback.
- `mushuctl vapid-export` writes this host's VAPID key to stdout, and warns on stderr when stdout is a terminal. `mushuctl vapid-import` reads one from stdin, so sharing a keypair is `mushuctl vapid-export | ssh otherhost '~/.local/bin/mushuctl vapid-import'`. The full remote path matters because a non-interactive ssh shell gets no login PATH. Import bounds its input and validates the exact P-256 scalar range, refuses symlink or non-regular state targets, creates `~/.config/mushu` when the host has never started Mushu, stages the new key at mode `0600`, backs up any key it replaces to `vapid.key.bak`, and moves retired subscriptions to `subscriptions.json.bak`. It restarts only when Mushu was already running; restart failure restores the previous key and subscriptions and returns an error. Importing a key the host already uses only repairs private-file permissions when needed and leaves subscriptions intact.
- `mushuctl with-herdr [args]` starts Mushu only when inactive, runs `herdr [args]` in the foreground, and stops Mushu on exit or a trapped signal only when it started Mushu.
- `mushuctl uninstall` removes Mushu from the host. It verifies one regular executable `mushu-server`/`mushuctl` sibling pair, guards every exact and recursive target against root, HOME, broad paths, and symlinks, prints every path it will delete, and asks for confirmation first. It refuses to run without a terminal unless given `--yes`. A service-manager error or failure to reach inactive and unloaded state aborts before deletion. It then removes the service unit and its backup, the verified binary pair, the token file, `~/.config/mushu`, the upload cache under `XDG_CACHE_HOME` or `~/.cache`, and only the exact launchd log file on macOS, never a directory derived from `MUSHU_LOG_FILE`. Removing `mushuctl` last is safe because the shell keeps its open descriptor to the running script. It does not touch the Tailscale Serve mapping, which may front other services, and prints `tailscale serve status` instead.
- `mushuctl help` shows command help.

## Operating modes

Always-on mode is best when the phone must reach Mushu regardless of whether a desktop Herdr client is open. Enable the user service and leave Mushu running; closing a Herdr client detaches that client but does not stop Herdr's persistent server.

`mushuctl with-herdr` is best when Mushu should exist only alongside one foreground Herdr client. Closing that client stops the temporary Mushu service, but still does not stop Herdr's persistent server; stop the Herdr server separately with Herdr's own command only when that is intentional.
