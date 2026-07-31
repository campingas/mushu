#!/bin/sh
# Install mushu-server and mushuctl from the latest GitHub release.
#   curl -fsSL https://raw.githubusercontent.com/campingas/mushu/main/install.sh | sh
# Override the destination with MUSHU_INSTALL_DIR, or the version with MUSHU_VERSION.

set -eu

REPO="campingas/mushu"
INSTALL_DIR="${MUSHU_INSTALL_DIR:-$HOME/.local/bin}"

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
  printf '\nNext:\n'
  printf '  (umask 077 && openssl rand -hex 24 > ~/.config/mushu-token)\n'
  printf '  tailscale serve --bg http://127.0.0.1:8422\n'
  printf '  mushuctl pair    # scan the QR with your phone\n'
}

main "$@"
