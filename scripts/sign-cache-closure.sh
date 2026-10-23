#!/usr/bin/env bash
#
# HiTech-OS — sign a store path and its closure with the cache signing key.
#
# Runs on the cache host (or any trusted signer that owns the private key).
# Intended to be called over SSH by scripts/publish-cache.sh and the
# .github/workflows/cache-publish.yml pipeline; it can also be used manually
# after a closure has been copied into the cache host's store.
#
# Usage:
#   scripts/sign-cache-closure.sh [-k KEY] PATH [PATH ...]
#
# Options:
#   -k, --key PATH   secret signing key (default: $HITECH_CACHE_SIGNING_KEY or
#                    /run/secrets/hitechos-cache-signing-key)
#   -h, --help       show this help
#
# Environment:
#   HITECH_CACHE_SIGNING_KEY  default key path
#
# Example (on the cache host):
#   scripts/sign-cache-closure.sh /nix/store/<hash>-nixos-system-patient-zero-<ver>

set -euo pipefail

NIX=(nix --extra-experimental-features "nix-command flakes")

die() { printf 'sign-cache-closure: error: %s\n' "$*" >&2; exit 1; }
log() { printf 'sign-cache-closure: %s\n' "$*" >&2; }

usage() { sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; }

key=${HITECH_CACHE_SIGNING_KEY:-/run/secrets/hitechos-cache-signing-key}
paths=()

while [ $# -gt 0 ]; do
  case "$1" in
    -k|--key) key=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; while [ $# -gt 0 ]; do paths+=("$1"); shift; done ;;
    -*) die "unknown option: $1" ;;
    *) paths+=("$1"); shift ;;
  esac
done

[ "${#paths[@]}" -gt 0 ] || { usage; exit 2; }
[ -r "$key" ] || die "signing key is not readable: $key"
command -v nix >/dev/null || die "nix is not in PATH"

for path in "${paths[@]}"; do
  [ -e "$path" ] || die "store path does not exist: $path"
  log "signing closure of $path"
  # Sign every closure member: Nix verifies each path it substitutes.
  "${NIX[@]}" path-info --recursive "$path" | "${NIX[@]}" store sign --key-file "$key" --stdin
  log "signed: $path"
done

log "done"
