#!/usr/bin/env bash
# Read-only checks for a running HiTech-OS target session.
#
# Usage:
#   SSH_KEY=~/.ssh/id_ed25519 \
#     ./scripts/e2e/verify-target-session.sh 127.0.0.1 2225 commander
#
# This does not launch a client and does not modify the target. It verifies the
# display manager, the HiTech-Shell process, the Wayland socket, and the active
# seat session.
set -euo pipefail

host=${1:?usage: verify-target-session.sh HOST PORT USER}
port=${2:?usage: verify-target-session.sh HOST PORT USER}
user=${3:?usage: verify-target-session.sh HOST PORT USER}
key=${SSH_KEY:-$HOME/.ssh/id_ed25519}

[[ -f "$key" ]] || { echo "SSH key not found: $key" >&2; exit 1; }

ssh_opts=(-p "$port" -i "$key"
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
  -o ConnectTimeout=10)

remote='set -e
echo "hostname: $(hostname)"
echo "kernel: $(uname -r)"
echo "generation: $(readlink -f /run/current-system)"
echo "display-manager: $(systemctl is-active display-manager)"
echo "networkmanager: $(systemctl is-active NetworkManager)"
echo "compositor: $(pgrep -f '^/run/current-system/sw/bin/hitech-shell$' || true)"
echo "wayland: $(ls /run/user/$(id -u)/wayland-* 2>/dev/null || true)"
echo "sessions:"
loginctl list-sessions --no-legend
'

output=$(ssh "${ssh_opts[@]}" "$user@$host" "$remote")
printf '%s\n' "$output"

grep -q '^display-manager: active$' <<<"$output"
grep -q '^networkmanager: active$' <<<"$output"
grep -q 'hitech-shell' <<<"$output"
grep -q 'wayland-[0-9]' <<<"$output"

echo "target session check: PASS"
