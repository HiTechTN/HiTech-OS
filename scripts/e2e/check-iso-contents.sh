#!/usr/bin/env bash
# Verify that a HiTech-OS ISO carries the expected hitech-installer payload.
#
# Usage:
#   ./scripts/e2e/check-iso-contents.sh NETINSTALLER.iso LIVE.iso [INSTALLER_STORE_BASENAME]
#
# The optional third argument is the expected store basename, for example:
#   i5g7srfdxxfgy3qmlwmmjigs4rb2b4sp-hitech-installer-2.0.0
#
# Mounting requires root; the script re-executes mount/umount through sudo when
# it is not already root.
set -euo pipefail

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage >&2
  exit 2
fi

net_iso=$1
live_iso=$2
expected=${3:-}

for iso in "$net_iso" "$live_iso"; do
  [[ -f "$iso" ]] || { echo "missing ISO: $iso" >&2; exit 1; }
done

if [[ $(id -u) -eq 0 ]]; then
  as_root() { "$@"; }
else
  as_root() { sudo -- "$@"; }
fi

work=$(mktemp -d)
iso_mnt="$work/iso"
store_mnt="$work/store"
mkdir -p "$iso_mnt" "$store_mnt"
iso_mounted=0
store_mounted=0

cleanup() {
  if [[ $store_mounted -eq 1 ]]; then as_root umount "$store_mnt" || true; fi
  if [[ $iso_mounted -eq 1 ]]; then as_root umount "$iso_mnt" || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

check_iso() {
  local label=$1 iso=$2 squash
  echo "== $label: $iso"
  as_root mount -o loop,ro "$iso" "$iso_mnt"
  iso_mounted=1

  squash=$(find "$iso_mnt" -maxdepth 1 -name 'nix-store.squashfs' -print -quit)
  if [[ -z "$squash" ]]; then
    echo "  no nix-store.squashfs found" >&2
    return 1
  fi

  as_root mount -o loop,ro "$squash" "$store_mnt"
  store_mounted=1

  local found
  found=$(find "$store_mnt" -maxdepth 1 -type d -name '*hitech-installer*' -printf '%f\n' | sort)
  if [[ -z "$found" ]]; then
    echo "  no hitech-installer store path found" >&2
    return 1
  fi
  echo "$found" | sed 's/^/  /'

  if [[ -n "$expected" ]] && ! grep -Fxq "$expected" <<<"$found"; then
    echo "  expected payload not found: $expected" >&2
    return 1
  fi

  as_root umount "$store_mnt"
  store_mounted=0
  as_root umount "$iso_mnt"
  iso_mounted=0
}

check_iso NetInstaller "$net_iso"
check_iso Live "$live_iso"

echo "ISO payload check: PASS"
