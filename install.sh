#!/bin/sh
# Install mushu-server and mushuctl from the latest GitHub release.
#   curl -fsSL https://raw.githubusercontent.com/campingas/mushu/main/install.sh | sh
# Override the destination with MUSHU_INSTALL_DIR, or the version with MUSHU_VERSION.

set -eu

REPO="campingas/mushu"
INSTALL_DIR="${MUSHU_INSTALL_DIR:-$HOME/.local/bin}"
# Whether this host is already set up, which is what decides the closing hint.
# Matches mushuctl's own default so a custom token path is not misread as unset.
TOKEN_FILE="${MUSHU_TOKEN_FILE:-$HOME/.config/mushu-token}"

die() {
  printf 'install.sh: %s\n' "$1" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

asset_name() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Darwin) os_part=macos ;;
    Linux) os_part=linux ;;
    *) die "unsupported operating system: $os (build from source instead)" ;;
  esac
  case "$arch" in
    arm64 | aarch64) arch_part=aarch64 ;;
    x86_64 | amd64) arch_part=x86_64 ;;
    *) die "unsupported architecture: $arch (build from source instead)" ;;
  esac
  printf 'mushu-server-%s-%s\n' "$os_part" "$arch_part"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    die "missing required command: sha256sum or shasum"
  fi
}

main() {
  need curl
  need uname

  asset="$(asset_name)"
  if [ "${MUSHU_VERSION:-}" = "" ]; then
    base="https://github.com/$REPO/releases/latest/download"
    version="latest"
  else
    base="https://github.com/$REPO/releases/download/$MUSHU_VERSION"
    version="$MUSHU_VERSION"
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  printf 'Downloading %s (%s)\n' "$asset" "$version"
  curl -fsSL "$base/$asset" -o "$tmp/mushu-server" ||
    die "no published release asset $asset at $base
Check https://github.com/$REPO/releases, or build from source with: cargo build --release"
  curl -fsSL "$base/mushuctl" -o "$tmp/mushuctl" || die "failed to download mushuctl from $base"

  # Verify before installing, not after.
  if curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"; then
    for file in "$asset:mushu-server" mushuctl:mushuctl; do
      want_name="${file%%:*}"
      local_name="${file##*:}"
      expected="$(grep " \{1,2\}$want_name\$" "$tmp/SHA256SUMS" | cut -d' ' -f1 || true)"
      [ -n "$expected" ] || die "no checksum for $want_name in SHA256SUMS"
      actual="$(sha256_of "$tmp/$local_name")"
      [ "$expected" = "$actual" ] ||
        die "checksum mismatch for $want_name (expected $expected, got $actual)"
    done
    printf 'Checksums verified\n'
  else
    die "could not download SHA256SUMS from $base"
  fi

  mkdir -p "$INSTALL_DIR"
  install -m 755 "$tmp/mushu-server" "$INSTALL_DIR/mushu-server"
  install -m 755 "$tmp/mushuctl" "$INSTALL_DIR/mushuctl"

  printf '\nInstalled to %s:\n  mushu-server\n  mushuctl\n' "$INSTALL_DIR"
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) printf '\nNote: %s is not in your PATH.\n' "$INSTALL_DIR" ;;
  esac
  case "$(uname -s)" in
    Darwin) unit_file="${MUSHU_LAUNCHD_PLIST:-$HOME/Library/LaunchAgents/dev.mushu.server.plist}" ;;
    Linux) unit_file="$HOME/.config/systemd/user/mushu.service" ;;
  esac
  if [ ! -e "$unit_file" ]; then
    printf '\nNext:\n'
    if [ -e "$TOKEN_FILE" ]; then
      printf '  mushuctl install-service         # validate token, install unit, then start\n'
    else
      printf '  mushuctl install-service         # generate token, install unit, then start\n'
    fi
    printf '  tailscale serve --bg http://127.0.0.1:8422\n'
    printf '  mushuctl pair                    # scan the QR with your phone\n'
  elif "$INSTALL_DIR/mushuctl" status 2>/dev/null | grep -q '^Mushu: active$'; then
    # `install` replaces the file rather than writing through it, so the live
    # process keeps running from the old inode and still serves the previous
    # version. Placement stays non-disruptive: the owner chooses when to drop
    # live sessions by restarting it.
    printf '\nThe running service is still on the previous version. When ready:\n'
    printf '  mushuctl restart\n'
  else
    printf '\nNext:\n'
    printf '  mushuctl start    # the installed service is not running\n'
  fi
}

main "$@"
