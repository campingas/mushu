#!/usr/bin/env bash

set -euo pipefail

repo_root="$(unset CDPATH; cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

pass_count=0
fail() { printf 'not ok %d - %s\n' "$((pass_count + 1))" "$1" >&2; exit 1; }
pass() { pass_count=$((pass_count + 1)); printf 'ok %d - %s\n' "$pass_count" "$1"; }
assert_file() { [[ -f "$1" ]] || fail "missing file: $1"; }
assert_no_file() { [[ ! -e "$1" ]] || fail "unexpected path: $1"; }
assert_contains() { grep -Fq -- "$2" "$1" || fail "$1 does not contain: $2"; }
assert_mode_600() {
  local mode
  mode="$(stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1")"
  [[ "${mode: -3}" == 600 ]] || fail "$1 mode is $mode, expected 600"
}
sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

make_fixture() {
  local name="$1" os="$2" root
  root="$scratch/$name"
  mkdir -p "$root/home/install space" "$root/fakebin" "$root/state"
  cp "$repo_root/scripts/mushuctl" "$root/home/install space/mushuctl"
  printf '#!/bin/sh\nexit 0\n' >"$root/home/install space/mushu-server"
  chmod 755 "$root/home/install space/mushuctl" "$root/home/install space/mushu-server"
  cat >"$root/fakebin/uname" <<EOF
#!/bin/sh
case "\${1:-}" in
  -m) printf '%s\n' x86_64 ;;
  *) printf '%s\n' '$os' ;;
esac
EOF
  cat >"$root/fakebin/herdr" <<'EOF'
#!/bin/sh
exit 0
EOF
  cat >"$root/fakebin/systemctl" <<'EOF'
#!/bin/sh
set -eu
[ "${1:-}" = --user ] && shift
printf '%s\n' "$*" >>"$FAKE_STATE/systemctl.log"
case "${1:-}" in
  is-active) [ -e "$FAKE_STATE/active" ] ;;
  is-enabled) [ -e "$FAKE_STATE/enabled" ] ;;
  daemon-reload) [ ! -e "$FAKE_STATE/fail-reload" ] ;;
  enable) touch "$FAKE_STATE/enabled" ;;
  start) touch "$FAKE_STATE/active" ;;
  restart)
    [ ! -e "$FAKE_STATE/fail-restart" ] || exit 1
    touch "$FAKE_STATE/active"
    ;;
  stop) rm -f "$FAKE_STATE/active" ;;
  disable)
    [ ! -e "$FAKE_STATE/fail-stop" ] || exit 1
    rm -f "$FAKE_STATE/active" "$FAKE_STATE/enabled"
    ;;
esac
EOF
  cat >"$root/fakebin/launchctl" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_STATE/launchctl.log"
case "${1:-}" in
  print)
    [ -e "$FAKE_STATE/loaded" ] || exit 1
    printf '    state = running\n'
    ;;
  bootstrap) touch "$FAKE_STATE/loaded" ;;
  bootout)
    [ ! -e "$FAKE_STATE/fail-stop" ] || exit 1
    rm -f "$FAKE_STATE/loaded"
    ;;
  kickstart) touch "$FAKE_STATE/loaded" ;;
esac
EOF
  chmod 755 "$root/fakebin/"*
  printf '%s\n' "$root"
}

run_ctl() {
  local root="$1"; shift
  HOME="$root/home" FAKE_STATE="$root/state" PATH="$root/fakebin:/usr/bin:/bin:/usr/sbin:/sbin" \
    "$root/home/install space/mushuctl" "$@"
}

test_render_and_binary_resolution() {
  local root output canonical_root
  root="$(make_fixture render Linux)"
  mkdir -p "$root/pathbin"
  printf '#!/bin/sh\nexit 0\n' >"$root/pathbin/mushu-server"
  chmod 755 "$root/pathbin/mushu-server"
  output="$root/rendered"
  (cd "$root/home/install space" && HOME="$root/home" FAKE_STATE="$root/state" \
    MUSHU_TOKEN_FILE="$root/home/key\$1" \
    PATH="$root/fakebin:$root/pathbin:/usr/bin:/bin" ./mushuctl install-service --print) >"$output"
  canonical_root="$(cd "$root" && pwd -P)"
  assert_contains "$output" "ExecStart=\"$canonical_root/home/install space/mushu-server\""
  assert_contains "$output" "Environment=\"MUSHU_TOKEN_FILE=$root/home/key\$1\""
  pass 'relative invocation resolves the canonical spaced sibling before PATH'
}

test_linux_install_service() {
  local root unit token restart_before restart_after rollback_root rollback_unit
  root="$(make_fixture linux-install Linux)"
  unit="$root/home/.config/systemd/user/mushu.service"
  token="$root/home/.config/mushu-token"
  run_ctl "$root" install-service >/dev/null
  assert_file "$unit"
  assert_file "$token"
  assert_mode_600 "$unit"
  assert_mode_600 "$token"
  if compgen -G "$(dirname "$unit")/.mushu-unit.*" >/dev/null; then fail 'staged unit was left behind'; fi
  restart_before="$(grep -c '^restart ' "$root/state/systemctl.log" || true)"
  run_ctl "$root" install-service >/dev/null
  restart_after="$(grep -c '^restart ' "$root/state/systemctl.log" || true)"
  [[ "$restart_before" == "$restart_after" ]] || fail 'idempotent install restarted the service'
  printf '# customized\n' >>"$unit"
  run_ctl "$root" install-service --force >/dev/null
  assert_contains "$unit.bak" '# customized'
  ! grep -Fq '# customized' "$unit" || fail 'force did not replace customized unit'
  assert_mode_600 "$unit"
  assert_mode_600 "$unit.bak"
  rollback_root="$(make_fixture linux-install-rollback Linux)"
  rollback_unit="$rollback_root/home/.config/systemd/user/mushu.service"
  touch "$rollback_root/state/fail-restart"
  if run_ctl "$rollback_root" install-service >/dev/null 2>&1; then fail 'failed first start was reported as success'; fi
  assert_no_file "$rollback_unit"
  assert_no_file "$rollback_root/state/enabled"
  pass 'Linux unit install is atomic, secure, idempotent, and force-backed-up'
}

test_token_validation() {
  local root token unit
  root="$(make_fixture token Linux)"
  token="$root/home/.config/mushu-token"
  unit="$root/home/.config/systemd/user/mushu.service"
  mkdir -p "$(dirname "$token")"
  printf 'short\n' >"$token"
  chmod 600 "$token"
  if run_ctl "$root" install-service >/dev/null 2>&1; then fail 'short token was accepted'; fi
  assert_no_file "$unit"
  printf '0123456789abcdef0123456789abcdef\n' >"$token"
  chmod 644 "$token"
  if run_ctl "$root" install-service >/dev/null 2>&1; then fail 'world-readable token was accepted'; fi
  assert_no_file "$unit"
  chmod 600 "$token"
  run_ctl "$root" install-service >/dev/null
  assert_file "$unit"
  pass 'token type, length, and Unix permissions are validated before unit install'
}

test_macos_install_and_uninstall_scope() {
  local root plist log sentinel canonical_root
  root="$(make_fixture macos Darwin)"
  plist="$root/home/Library/LaunchAgents/dev.mushu.server.plist"
  log="$root/home/Library/Logs/custom logs/mushu-server.log"
  if HOME="$root/home" FAKE_STATE="$root/state" PATH="$root/fakebin:/usr/bin:/bin:/usr/sbin:/sbin" \
    MUSHU_LOG_FILE="$root/home" "$root/home/install space/mushuctl" uninstall --yes >/dev/null 2>&1; then
    fail 'uninstall accepted HOME as a log target'
  fi
  assert_file "$root/home/install space/mushu-server"
  HOME="$root/home" FAKE_STATE="$root/state" PATH="$root/fakebin:/usr/bin:/bin:/usr/sbin:/sbin" \
    MUSHU_LOG_FILE="$log" "$root/home/install space/mushuctl" install-service >/dev/null
  if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$plist" >/dev/null
  fi
  canonical_root="$(cd "$root" && pwd -P)"
  assert_contains "$plist" "<string>$canonical_root/home/install space/mushu-server</string>"
  mkdir -p "$(dirname "$log")"
  printf 'log\n' >"$log"
  sentinel="$(dirname "$log")/keep-me"
  printf 'sentinel\n' >"$sentinel"
  HOME="$root/home" FAKE_STATE="$root/state" PATH="$root/fakebin:/usr/bin:/bin:/usr/sbin:/sbin" \
    MUSHU_LOG_FILE="$log" "$root/home/install space/mushuctl" uninstall --yes >/dev/null
  assert_file "$sentinel"
  assert_no_file "$log"
  assert_no_file "$plist"
  pass 'macOS plist lints and uninstall removes only the exact custom log file'
}

test_uninstall_stop_failure() {
  local root unit sentinel
  root="$(make_fixture stop-failure Linux)"
  run_ctl "$root" install-service >/dev/null
  unit="$root/home/.config/systemd/user/mushu.service"
  sentinel="$root/home/.config/mushu/sentinel"
  mkdir -p "$(dirname "$sentinel")"
  printf 'keep\n' >"$sentinel"
  touch "$root/state/fail-stop"
  if run_ctl "$root" uninstall --yes >/dev/null 2>&1; then fail 'uninstall ignored service stop failure'; fi
  assert_file "$unit"
  assert_file "$sentinel"
  assert_file "$root/home/install space/mushu-server"
  pass 'service stop failure aborts uninstall before deletion'
}

test_vapid_import() {
  local root config key old valid order plus
  root="$(make_fixture vapid Linux)"
  config="$root/home/.config/mushu"
  key="$config/vapid.key"
  old='AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAE'
  valid='_____wAAAAD__________7zm-q2nF56E87nKwvxjJVA'
  order='_____wAAAAD__________7zm-q2nF56E87nKwvxjJVE'
  plus='_____wAAAAD__________7zm-q2nF56E87nKwvxjJVI'
  mkdir -p "$config"
  printf '%s' "$old" >"$key"
  chmod 600 "$key"
  printf 'subscriptions\n' >"$config/subscriptions.json"
  printf '%s\n' "$valid" | run_ctl "$root" vapid-import >/dev/null
  [[ "$(<"$key")" == "$valid" ]] || fail 'valid scalar was not installed'
  assert_mode_600 "$key"
  assert_mode_600 "$config/vapid.key.bak"
  assert_contains "$config/vapid.key.bak" "$old"
  assert_contains "$config/subscriptions.json.bak" subscriptions
  assert_no_file "$config/subscriptions.json"
  for invalid in 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA' "$order" "$plus"; do
    if printf '%s\n' "$invalid" | run_ctl "$root" vapid-import >/dev/null 2>&1; then
      fail "invalid scalar accepted: $invalid"
    fi
    [[ "$(<"$key")" == "$valid" ]] || fail 'invalid scalar changed the key'
  done
  chmod 644 "$key" "$config/vapid.key.bak"
  printf '%s\n' "$valid" | run_ctl "$root" vapid-import >/dev/null
  assert_mode_600 "$key"
  assert_mode_600 "$config/vapid.key.bak"
  if printf '%0129d' 0 | run_ctl "$root" vapid-import >/dev/null 2>&1; then fail 'oversized VAPID input accepted'; fi
  mv "$key" "$config/real-key"
  ln -s "$config/real-key" "$key"
  if printf '%s\n' "$old" | run_ctl "$root" vapid-import >/dev/null 2>&1; then fail 'symlink VAPID target accepted'; fi
  [[ "$(<"$config/real-key")" == "$valid" ]] || fail 'symlink refusal changed its referent'
  pass 'VAPID import enforces bounded exact P-256 scalar validation and secure replacement'
}

test_vapid_restart_rollback() {
  local root config old new
  root="$(make_fixture vapid-rollback Linux)"
  config="$root/home/.config/mushu"
  old='AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAE'
  new='AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAI'
  mkdir -p "$config"
  printf '%s' "$old" >"$config/vapid.key"
  chmod 600 "$config/vapid.key"
  printf 'subscriptions\n' >"$config/subscriptions.json"
  touch "$root/state/active" "$root/state/fail-restart"
  if printf '%s\n' "$new" | run_ctl "$root" vapid-import >/dev/null 2>&1; then fail 'restart failure was reported as success'; fi
  [[ "$(<"$config/vapid.key")" == "$old" ]] || fail 'restart failure did not restore key'
  assert_contains "$config/subscriptions.json" subscriptions
  pass 'VAPID restart failure restores the previous key and subscriptions'
}

test_installer_guidance() {
  local root fixtures install_dir output asset server_sum ctl_sum
  root="$(make_fixture installer Linux)"
  fixtures="$root/release"
  install_dir="$root/home/bin"
  asset='mushu-server-linux-x86_64'
  mkdir -p "$fixtures"
  printf '#!/bin/sh\nexit 0\n' >"$fixtures/$asset"
  cp "$repo_root/scripts/mushuctl" "$fixtures/mushuctl"
  chmod 755 "$fixtures/$asset" "$fixtures/mushuctl"
  server_sum="$(sha256_file "$fixtures/$asset")"
  ctl_sum="$(sha256_file "$fixtures/mushuctl")"
  printf '%s  %s\n%s  mushuctl\n' "$server_sum" "$asset" "$ctl_sum" >"$fixtures/SHA256SUMS"
  cat >"$root/fakebin/curl" <<'EOF'
#!/bin/sh
set -eu
url=''
dest=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) dest="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
cp "$FAKE_RELEASE/${url##*/}" "$dest"
EOF
  chmod 755 "$root/fakebin/curl"
  output="$root/install.out"
  HOME="$root/home" FAKE_STATE="$root/state" FAKE_RELEASE="$fixtures" \
    MUSHU_INSTALL_DIR="$install_dir" PATH="$root/fakebin:/usr/bin:/bin" sh "$repo_root/install.sh" >"$output"
  assert_contains "$output" 'mushuctl install-service         # generate token, install unit, then start'
  mkdir -p "$root/home/.config/systemd/user"
  printf '[Unit]\n' >"$root/home/.config/systemd/user/mushu.service"
  printf '0123456789abcdef\n' >"$root/home/.config/mushu-token"
  chmod 600 "$root/home/.config/mushu-token"
  touch "$root/state/active"
  HOME="$root/home" FAKE_STATE="$root/state" FAKE_RELEASE="$fixtures" \
    MUSHU_INSTALL_DIR="$install_dir" PATH="$root/fakebin:/usr/bin:/bin" sh "$repo_root/install.sh" >"$output"
  assert_contains "$output" 'The running service is still on the previous version. When ready:'
  ! grep -q '^restart ' "$root/state/systemctl.log" || fail 'installer restarted the service'
  pass 'installer remains placement-only and distinguishes first setup from restart guidance'
}

printf '1..8\n'
test_render_and_binary_resolution
test_linux_install_service
test_token_validation
test_macos_install_and_uninstall_scope
test_uninstall_stop_failure
test_vapid_import
test_vapid_restart_rollback
test_installer_guidance
