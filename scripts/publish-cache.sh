#!/usr/bin/env bash
#
# HiTech-OS — publish system closures to the Harmonia binary cache.
#
# Builds one or more flake targets, optionally signs them with a local secret
# key, uploads the closure to the cache host over SSH, and optionally lets the
# cache host sign the closure with the key that lives there
# (default: /run/secrets/hitechos-cache-signing-key).
#
# Security model (see docs/audit/PHASE8-NETINSTALLER-RUST-DE.md §5):
#   - The cache host owns the private signing key. CI never receives it.
#   - CI copies closures over a restricted SSH key, then the cache host signs
#     them. Clients verify the signatures with the matching public key.
#   - A trusted builder that does have the key can sign locally with --sign-key.
#
# Usage:
#   scripts/publish-cache.sh [options] [target ...]
#
# Targets:
#   patient-zero   nixosConfigurations.patient-zero system closure (default)
#   hitech-shell   Rust DE package (default)
#   netinstaller   NetInstaller ISO
#   iso            legacy full ISO
#   <installable>  any flake installable passed through to `nix build`
#
# Options:
#   --repo PATH          repository root (default: git toplevel)
#   --host user@host     cache host SSH target (or HITECH_CACHE_HOST)
#   --sign-key PATH      local secret key; signs before upload
#   --remote-key PATH    key path on the cache host; signs after upload
#   --targets "a b"      space-separated targets (default: patient-zero hitech-shell)
#   --no-push            build (and optionally sign) locally, skip the network
#   --no-sign            never sign, even if a key was supplied
#   --dry-run            print what would be built/pushed
#   -h, --help           show this help
#
# Environment:
#   HITECH_CACHE_HOST         cache host SSH target ("user@host")
#   HITECH_CACHE_SIGNING_KEY  local secret key (trusted builders only)
#   HITECH_CACHE_REMOTE_KEY   key path on the cache host
#   HITECH_CACHE_TARGETS      space-separated default targets
#   HITECH_REPO_ROOT          override repository root
#
# Examples:
#   # trusted builder with the key mounted locally
#   scripts/publish-cache.sh --sign-key /secure/hitechos-cache-1.secret
#
#   # CI: push unsigned, let the cache host sign
#   HITECH_CACHE_HOST=cache-publisher@cache.hitechos.tn \
#     scripts/publish-cache.sh
#
#   # build and sign locally, no network
#   scripts/publish-cache.sh --no-push --sign-key /secure/hitechos-cache-1.secret

set -euo pipefail

NIX=(nix --extra-experimental-features "nix-command flakes")

die() { printf 'publish-cache: error: %s\n' "$*" >&2; exit 1; }
log() { printf 'publish-cache: %s\n' "$*" >&2; }

usage() { sed -n '2,53p' "$0" | sed 's/^# \{0,1\}//'; }

repo_root() {
  if [ -n "${HITECH_REPO_ROOT:-}" ]; then
    printf '%s' "$HITECH_REPO_ROOT"
    return
  fi
  if root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null); then
    printf '%s' "$root"
    return
  fi
  printf '%s' "$(cd "$(dirname "$0")/.." && pwd)"
}

target_attr() {
  case "$1" in
    patient-zero) printf '%s' '.#nixosConfigurations.patient-zero.config.system.build.toplevel' ;;
    hitech-shell) printf '%s' '.#hitech-shell' ;;
    netinstaller) printf '%s' '.#nixosConfigurations.netinstaller.config.system.build.isoImage' ;;
    iso) printf '%s' '.#nixosConfigurations.iso.config.system.build.isoImage' ;;
    *) printf '%s' "$1" ;;
  esac
}

# Sign every path in the closure. Nix verifies each substituted path, so every
# closure member needs a signature, not only the toplevel.
sign_closure() {
  local key=$1 path=$2
  [ -r "$key" ] || die "signing key is not readable: $key"
  log "signing closure of $path with $key"
  "${NIX[@]}" path-info --recursive "$path" | "${NIX[@]}" store sign --key-file "$key" --stdin
}

push_closure() {
  local host=$1 path=$2
  log "pushing $path -> ssh-ng://$host"
  "${NIX[@]}" copy --to "ssh-ng://$host" --no-check-sigs "$path"
}

sign_closure_remote() {
  local host=$1 key=$2 path=$3
  log "signing $path on $host with $key"
  ssh -o BatchMode=yes -o ConnectTimeout=15 "$host" \
    "nix --extra-experimental-features 'nix-command flakes' path-info --recursive '$path' | nix --extra-experimental-features 'nix-command flakes' store sign --key-file '$key' --stdin"
}

verify_remote() {
  local host=$1 path=$2
  "${NIX[@]}" path-info --store "ssh-ng://$host" "$path" >/dev/null
  log "verified present in cache: $path"
}

main() {
  local repo="" host="" sign_key="" remote_key="" no_push=0 no_sign=0 dry_run=0
  local -a targets=()

  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) repo=$2; shift 2 ;;
      --host) host=$2; shift 2 ;;
      --sign-key) sign_key=$2; shift 2 ;;
      --remote-key) remote_key=$2; shift 2 ;;
      --targets) read -r -a targets <<< "$2"; shift 2 ;;
      --no-push) no_push=1; shift ;;
      --no-sign) no_sign=1; shift ;;
      --dry-run) dry_run=1; shift ;;
      -h|--help) usage; exit 0 ;;
      --) shift; while [ $# -gt 0 ]; do targets+=("$1"); shift; done ;;
      -*) die "unknown option: $1" ;;
      *) targets+=("$1"); shift ;;
    esac
  done

  repo=${repo:-$(repo_root)}
  host=${host:-${HITECH_CACHE_HOST:-}}
  sign_key=${sign_key:-${HITECH_CACHE_SIGNING_KEY:-}}
  remote_key=${remote_key:-${HITECH_CACHE_REMOTE_KEY:-/run/secrets/hitechos-cache-signing-key}}
  host=${host#ssh-ng://}

  if [ "${#targets[@]}" -eq 0 ]; then
    read -r -a targets <<< "${HITECH_CACHE_TARGETS:-patient-zero hitech-shell}"
  fi

  [ -d "$repo" ] || die "repository root not found: $repo"
  command -v nix >/dev/null || die "nix is not in PATH"

  if [ "$no_push" -eq 0 ] && [ "$dry_run" -eq 0 ] && [ -z "$host" ]; then
    die "no cache host: pass --host user@host or set HITECH_CACHE_HOST (or use --no-push)"
  fi

  log "repository: $repo"
  log "targets:    ${targets[*]}"
  [ -n "$host" ] && log "cache host: $host"
  [ "$no_push" -eq 1 ] && log "mode:       build only (--no-push)"
  [ "$no_sign" -eq 1 ] && log "mode:       signing disabled (--no-sign)"

  cd "$repo"

  local name attr out
  for name in "${targets[@]}"; do
    attr=$(target_attr "$name")
    log "building $name ($attr)"

    if [ "$dry_run" -eq 1 ]; then
      printf '  nix build %s --no-link --print-out-paths\n' "$attr"
      continue
    fi

    out=$("${NIX[@]}" build "$attr" --no-link --print-out-paths -L)
    [ -n "$out" ] || die "build produced no output for $name"
    log "built $out"

    if [ "$no_sign" -eq 0 ] && [ -n "$sign_key" ]; then
      sign_closure "$sign_key" "$out"
    fi

    if [ "$no_push" -eq 0 ]; then
      push_closure "$host" "$out"
      if [ "$no_sign" -eq 0 ] && [ -z "$sign_key" ]; then
        sign_closure_remote "$host" "$remote_key" "$out"
      fi
      verify_remote "$host" "$out"
    fi
  done

  log "done"
}

main "$@"
