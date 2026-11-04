#!/usr/bin/env bash
# Unit/integration tests for bootstrap/hitech-bootstrap.sh.
set -Eeuo pipefail

TEST_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$TEST_DIR/../.." && pwd)"
SCRIPT="$REPO_ROOT/bootstrap/hitech-bootstrap.sh"
PASS=0
FAIL=0

assert_eq() {
  local expected=$1 actual=$2 message=$3
  if [[ "$expected" == "$actual" ]]; then
    PASS=$((PASS + 1)); printf 'ok - %s\n' "$message"
  else
    FAIL=$((FAIL + 1)); printf 'not ok - %s (expected=%q actual=%q)\n' "$message" "$expected" "$actual"
  fi
}

assert_contains() {
  local haystack=$1 needle=$2 message=$3
  if [[ "$haystack" == *"$needle"* ]]; then
    PASS=$((PASS + 1)); printf 'ok - %s\n' "$message"
  else
    FAIL=$((FAIL + 1)); printf 'not ok - %s (missing %q)\n' "$message" "$needle"
  fi
}

assert_fail() {
  local message=$1; shift
  if "$@" >/dev/null 2>&1; then
    FAIL=$((FAIL + 1)); printf 'not ok - %s\n' "$message"
  else
    PASS=$((PASS + 1)); printf 'ok - %s\n' "$message"
  fi
}

# Load functions without running main.
# shellcheck source=../hitech-bootstrap.sh
. "$SCRIPT"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- profile matrix ---------------------------------------------------------
PROFILE=orion
assert_contains "$(profile_packages)" "hitech-shell" "orion includes hitech-shell"
assert_contains "$(profile_packages)" "influxdb2" "orion includes telemetry"
PROFILE=minimal
assert_contains "$(profile_packages)" "ai-daemon" "minimal includes ai-daemon"
assert_contains "$(service_units)" "mosquitto" "minimal includes mosquitto"
if [[ "$(service_units)" == *"hitech-store"* ]]; then
  FAIL=$((FAIL + 1)); printf 'not ok - minimal excludes hitech-store service\n'
else
  PASS=$((PASS + 1)); printf 'ok - minimal excludes hitech-store service\n'
fi
if [[ "$(profile_packages)" == *"hitech-shell"* ]]; then
  FAIL=$((FAIL + 1)); printf 'not ok - minimal excludes hitech-shell\n'
else
  PASS=$((PASS + 1)); printf 'ok - minimal excludes hitech-shell\n'
fi
assert_fail "invalid profile is rejected" valid_profile "gaming"

# --- OS detection -----------------------------------------------------------
cat >"$TMP/os-release" <<'RELEASE'
NAME="NixOS"
ID=nixos
ID_LIKE=""
VERSION_ID="25.05"
RELEASE
unset WSL_DISTRO_NAME || true
HITECH_OS_RELEASE="$TMP/os-release" detect_os
assert_eq "nixos" "$DISTRO_FAMILY" "detects NixOS family"
assert_eq "NixOS" "$OS_NAME" "reads os-release name"

cat >"$TMP/ubuntu-release" <<'RELEASE'
NAME="Ubuntu"
ID=ubuntu
ID_LIKE=debian
VERSION_ID="24.04"
RELEASE
HITECH_OS_RELEASE="$TMP/ubuntu-release" detect_os
assert_eq "debian" "$DISTRO_FAMILY" "detects Debian family via ID_LIKE"

cat >"$TMP/arch-release" <<'RELEASE'
NAME="Arch Linux"
ID=arch
ID_LIKE=""
VERSION_ID="rolling"
RELEASE
HITECH_OS_RELEASE="$TMP/arch-release" detect_os
assert_eq "arch" "$DISTRO_FAMILY" "detects Arch family"

WSL_DISTRO_NAME="Ubuntu-24.04" HITECH_OS_RELEASE="$TMP/ubuntu-release" detect_os
assert_eq "wsl2" "$OS_ID" "detects WSL2"
unset WSL_DISTRO_NAME || true

# --- hardware detection -----------------------------------------------------
cat >"$TMP/cpuinfo" <<'CPU'
vendor_id	: GenuineIntel
model name	: Test CPU @ 2.00GHz
CPU
HITECH_CPUINFO="$TMP/cpuinfo" detect_hardware
assert_eq "GenuineIntel" "$CPU_VENDOR" "reads CPU vendor"
assert_eq "Test CPU @ 2.00GHz" "$CPU_MODEL" "reads CPU model"

# --- safe writes ------------------------------------------------------------
ROOT="$TMP/root"
DRY_RUN=0
mkdir -p "$ROOT"
write_file /etc/hitechos/test.conf 0640 "alpha\nbeta\n"
assert_eq $'alpha\nbeta' "$(cat "$ROOT/etc/hitechos/test.conf")" "write_file expands newlines"
write_if_missing /etc/hitechos/secret.env 0600 "secret=first\n"
write_if_missing /etc/hitechos/secret.env 0600 "secret=second\n"
assert_eq $'secret=first' "$(cat "$ROOT/etc/hitechos/secret.env")" "write_if_missing preserves existing secrets"
assert_eq "640" "$(stat -c '%a' "$ROOT/etc/hitechos/test.conf")" "write_file applies mode"
write_file /etc/hitechos/test.conf 0640 "alpha\nbeta\n"
assert_eq $'alpha\nbeta' "$(cat "$ROOT/etc/hitechos/test.conf")" "write_file is idempotent"

# --- source resolution ------------------------------------------------------
mkdir -p "$TMP/source"
printf '{}\n' >"$TMP/source/flake.nix"
SOURCE_DIR="$TMP/source"
FLAKE_REF=""
resolve_source
assert_eq "path:$TMP/source" "$FLAKE_REF" "resolve_source emits a path flake ref"

# --- dry-run never writes ---------------------------------------------------
ROOT="$TMP/dry-root"
OUT=$(bash "$SCRIPT" --dry-run --profile minimal --root "$ROOT" 2>&1)
assert_contains "$OUT" "dry-run complete" "dry-run completes"
assert_contains "$OUT" "source:" "dry-run reports source"
assert_eq "" "$(find "$ROOT" -mindepth 1 -print -quit 2>/dev/null || true)" "dry-run creates no files"

# --- dry-run output has no host modifications ------------------------------
assert_contains "$OUT" "packages:" "dry-run reports package plan"
assert_contains "$OUT" "services:" "dry-run reports service plan"

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
(( FAIL == 0 ))
