# mushuctl

`mushuctl` controls the Mushu user service on macOS launchd and Linux systemd without printing the service environment or token.

## Install

Build and install the server and controller:

```sh
cargo build --release
install -d "$HOME/.local/bin" "$HOME/.config"
install -m 755 target/release/mushu-server "$HOME/.local/bin/mushu-server"
install -m 755 scripts/mushuctl "$HOME/.local/bin/mushuctl"
(umask 077 && openssl rand -hex 24 > "$HOME/.config/mushu-token")
```

The token file must be a regular file, contain at least 16 characters, and have no group or other permissions on Unix. `mushu-server` reads `MUSHU_TOKEN_FILE` directly, trims surrounding whitespace, and gives it precedence over the compatible `MUSHU_TOKEN` environment variable.

### macOS launchd

The launchd template uses placeholders because launchd does not expand `$HOME` in plist values. Install a rendered copy with absolute paths:

```sh
install -d "$HOME/Library/LaunchAgents" "$HOME/Library/Logs/mushu"
MUSHU_SERVICE_PATH="$(dirname "$(command -v herdr)"):$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
sed \
  -e "s|@MUSHU_SERVER@|$HOME/.local/bin/mushu-server|g" \
  -e "s|@MUSHU_TOKEN_FILE@|$HOME/.config/mushu-token|g" \
  -e "s|@MUSHU_LOG_FILE@|$HOME/Library/Logs/mushu/mushu-server.log|g" \
  -e "s|@MUSHU_PATH@|$MUSHU_SERVICE_PATH|g" \
  services/launchd/dev.mushu.server.plist > "$HOME/Library/LaunchAgents/dev.mushu.server.plist"
mushuctl start
```

The renderer puts the installed `herdr` directory on the service PATH. Run `plutil -lint "$HOME/Library/LaunchAgents/dev.mushu.server.plist"` after rendering if `plutil` is available.

### Linux systemd

The systemd user template assumes the server and token paths installed above and includes common user-local Herdr locations in its PATH:

```sh
install -d "$HOME/.config/systemd/user"
install -m 644 services/systemd/mushu.service "$HOME/.config/systemd/user/mushu.service"
systemctl --user daemon-reload
systemctl --user enable mushu.service
mushuctl start
```

Enable linger with `loginctl enable-linger "$USER"` only if Mushu must start before login and remain available after logout; this system-level policy may require administrator approval.

## Commands

- `mushuctl start` starts Mushu.
- `mushuctl stop` stops Mushu and its open web terminal sessions; it does not stop the persistent Herdr server.
- `mushuctl restart` restarts Mushu.
- `mushuctl status` reports only sanitized active state, plus launchd loaded state on macOS.
- `mushuctl logs` follows the launchd log file or systemd journal.
- `mushuctl pair` prints a QR code that signs a phone in to this host. It defaults `MUSHU_TOKEN_FILE` to `$HOME/.config/mushu-token` and resolves the public URL from the Tailscale Serve mapping that proxies this host's bind address; set `MUSHU_URL` when there is no such mapping. The QR carries the token in the URL fragment, and the URL and token are printed underneath as a fallback.
- `mushuctl with-herdr [args]` starts Mushu only when inactive, runs `herdr [args]` in the foreground, and stops Mushu on exit or a trapped signal only when it started Mushu.
- `mushuctl help` shows command help.

## Operating modes

Always-on mode is best when the phone must reach Mushu regardless of whether a desktop Herdr client is open. Enable the user service and leave Mushu running; closing a Herdr client detaches that client but does not stop Herdr's persistent server.

`mushuctl with-herdr` is best when Mushu should exist only alongside one foreground Herdr client. Closing that client stops the temporary Mushu service, but still does not stop Herdr's persistent server; stop the Herdr server separately with Herdr's own command only when that is intentional.
