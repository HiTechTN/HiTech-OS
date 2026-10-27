# Phase 8 — NetInstaller minimal + HiTech-Shell (Rust Wayland DE)

Status: architecture + scaffold landed; 8.1 Rust GUI installer is built and boots in QEMU.
Date: 2026-10-23 (updated 2026-10-27)
Repo: `github:HiTechTN/HiTech-OS`
Base commit: `2053f58` (Phase 7) + Phase 8 working tree

This document is the master reference for the Phase 8 pivot. It is deliberately
written so that a future agent can resume without re-reading the whole repo.

---

## 0. Phase 8.1 addendum — Rust GUI replaces Calamares (2026-10-27)

The Calamares/Qt installer described in the older sections of this report has
been replaced end-to-end by `hitech-installer`, a Rust `eframe`/`egui` kiosk app:

- **Package**: `hitech-installer/default.nix` (`rustPlatform.buildRustPackage`,
  `Cargo.lock` pinned, GL/Wayland/X11 runtime wrapper) and
  `packages.x86_64-linux.hitech-installer` in the flake.
- **Kiosk**: `services.cage.program` points at the Rust binary; Calamares and
  its Qt/KDE dependencies are removed from the live closure.
- **Flow**: Welcome → Configuration → Installing → Done; French/English labels;
  anonymous identity form; WiFi (`nmcli`) and disk (`lsblk`) discovery; live
  terminal launcher (`foot`); GPT/ESP/ext4 partitioning; `nixos-generate-config`;
  pinned profile fetch; `nixos-install` with cache substituters.
- **Privilege**: the GUI runs as `nixos` and uses passwordless `sudo` scoped to
  the live ISO (`security.sudo.wheelNeedsPassword = false`). The target profile
  keeps password-protected sudo and the operator password created by the form.
- **No hardcoded live password**: the live user has an empty password; the
  target identity is entirely supplied by the GUI.
- **Measured ISO**: **823 MiB** (862,978,048 bytes), above the ~800 MiB target.
  Built with `nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage`.
  The increase over the earlier 692 MiB image is the restored base repair/login
  toolkit plus the targeted amdgpu firmware, not Calamares.
- **Verified**: `cargo check` clean; `nix build .#hitech-installer` succeeds;
  `nix flake check --no-build` passes; the ISO builds and boots in QEMU with
  `cage-tty1` active and the Rust Welcome screen rendered.
- **Still pending**: complete a full install from the ISO (pinned profile fetch
  and cache substitution), then validate physical hardware.

### 0.1 Live-login regression fix (2026-10-27)

The first 8.1 ISO booted to a black screen. `os/netinstaller.nix` used
`environment.systemPackages = lib.mkForce ...` and
`environment.defaultPackages = lib.mkForce [ ]`, which removed
`bashInteractive` and the base login/tty packages. `cage-tty1` and the getty
login could not exec `/run/current-system/sw/bin/bash`, so the kiosk never
appeared.

Fixes:
- Removed both `lib.mkForce` overrides. The base profile now merges with the
  live tool set; `bashInteractive` and `coreutils` are explicit safety nets.
- Wrapped `hitech-installer` with a runtime PATH for `bash`, `util-linux`,
  `NetworkManager`, `iproute2`, `parted`, `dosfstools`, `e2fsprogs`,
  `nixos-install-tools`, `git`, `foot`, `nix`, and `/run/wrappers/bin`.
- Cage env: `WLR_NO_HARDWARE_CURSORS=1`, `WLR_RENDERER_ALLOW_SOFTWARE=1`,
  `XDG_RUNTIME_DIR=/run/user/1000`. `LIBGL_ALWAYS_SOFTWARE=1` is set by the
  package wrapper only when neither `/dev/dri/card0` nor
  `/dev/dri/renderD128` exists, preserving `amdgpu` acceleration.
- Added `amdgpu`/`virtio_gpu` to `boot.initrd.kernelModules`,
  `hardware.graphics.enable = true`, and the `video`/`render`/`input`/`tty`
  groups.
- `services.getty.autologinUser = "nixos"` plus an enabled
  `serial-getty@ttyS0` autologin.
- Added a 90 MiB `amdgpu` firmware subset (Polaris/RX 580 included) to
  `hardware.firmware`; full `linux-firmware` is 1.9 GiB.

Verified in QEMU (KVM, `-vga virtio`, `-serial stdio`):
- `/run/current-system/sw/bin/bash` resolves to `bash-interactive-5.3p15` and
  `BASH_EXIT=0`.
- Serial `ttyS0` autologins as `nixos`; groups include
  `wheel,tty,video,networkmanager,input,render`.
- `systemctl is-active cage-tty1` is `active`; the framebuffer shows the Rust
  installer Welcome screen at 1024x768 (non-black).
- `initrd` contains `amdgpu/polaris10_*.bin.zst` firmware.

The older Calamares-specific sections below are retained as historical design
notes; where they conflict with this addendum, this addendum wins.

---

## 1. Executive summary

Phase 7 made the legacy full graphical ISO (GNOME + Flatpak + Firefox +
Calamares, ~1.9 GiB measured) install HiTech-OS services correctly. Phase 8
replaces that image with two separate, smaller artefacts:

1. **NetInstaller** — a minimal `cage` kiosk ISO (`nixosConfigurations.netinstaller`)
   running the Rust `hitech-installer` (eframe/egui) GUI. It partitions the
   target, clones the HiTech-OS configuration at the revision baked into the
   ISO, writes an anonymous operator identity, and installs the target closure
   from the HiTech-Store binary cache.
2. **HiTech-Shell** — the Rust/Wayland desktop environment, scaffolded as a
   Smithay compositor + a declarative shell UI with three NixOS layout
   profiles (`hybrid`, `mac`, `windows`).

Two integration seams are introduced:

- `services.hitech-cache` — Harmonia binary-cache server plus client options.
- `services.hitech-shell` — declarative contract for the Rust shell.

Measured state after the scaffold:

| Artefact | Command | Result |
|---|---|---|
| NetInstaller ISO | `nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage` | built; **823 MiB** in 8.1 (see §0) |
| HiTech-Shell scaffold | `nix build .#hitech-shell` / `nix run .#hitech-shell` | built and runs (`hybrid` layout by default) |
| Flake evaluation | `nix flake check --no-build` | all checks passed |
| Legacy full ISO | unchanged | still available as `.#nixosConfigurations.iso` |

The ~800 MiB target is **not yet met** (823 MiB). The increase from 692 MiB is
from restoring the base login/repair packages and adding the amdgpu firmware
subset; the Calamares/Qt/KDE runtime is gone. §4.4 records the historical
reduction options; the remaining size trade-off is an explicit open decision.

---

## 2. Goals and non-goals

### Goals

- NetInstaller ISO that is small enough to download quickly and audit easily.
- Install the **same** HiTech-OS profile as patient-zero, not a generic NixOS.
- Pull pre-built closures from a signed, read-only binary cache instead of
  rebuilding the full stack on every target.
- Keep the installer reproducible: the ISO pins both the HiTech-OS revision and
  the Nixpkgs revision.
- Give the future Rust desktop environment a stable NixOS contract **before**
  writing the compositor/UI code.
- Keep all secrets out of the repository and out of the ISO.

### Non-goals (this phase)

- Writing the Smithay compositor or the Slint/Iced shell UI.
- A fully validated physical-hardware install (requires a real machine/VM run).
- Replacing the HiTech-Store FastAPI service; the cache is a **sibling service**
  on the same host.
- Supporting all possible live hardware. The NetInstaller explicitly trades
  `hardware.enableAllHardware` for size; wired Ethernet is the supported live
  network path.

---

## 3. Current baseline (what existed before Phase 8)

- `flake.nix` exposed `patient-zero` (headless) and `iso` (full graphical).
- `os/iso.nix` imported `installation-cd-graphical-base.nix` and
  `os/calamares-hitechos.nix`; the latter patched Calamares to clone the repo
  and run `nixos-install` from the live environment.
- `os/installation.nix` is the shared target profile:
  `common.nix`, `ecosystem.nix`, `services/hitech-store.nix`,
  `services/genio.nix`, `services/secrets.nix`.
- No binary cache, no minimal installer, no `apps/hitech-shell`, and no
  `docs/audit/` directory at the repo root.

---

## 4. NetInstaller architecture

### 4.1 Flake output

```nix
nixosConfigurations.netinstaller = nixpkgs.lib.nixosSystem {
  system = "x86_64-linux";
  specialArgs.hitechosRevision = self.rev or "main";
  modules = [
    "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-base.nix"
    ./os/netinstaller.nix
  ];
};
```

`self.rev` is baked into the ISO as the default
`hitechos.netinstaller.repoRef`. A clean release build therefore installs
exactly the commit it was built from. A dirty-tree build falls back to `main`
and must not be published as a release.

### 4.2 Boot and install sequence

```
UEFI/BIOS
  └─ NixOS initrd (curated storage/USB/virtio modules)
       └─ live squashfs
            └─ graphical.target
                 └─ cage-tty1.service  (Wayland kiosk, user=nixos)
                      └─ calamares
                           ├─ show: welcome, locale, keyboard, users, partition, summary
                           ├─ exec: partition, mount
                           ├─ exec: nixos            (stock NixOS job: base install)
                           ├─ exec: shellprocess@hitechos
                           │     ├─ check cache https://<cache>/nix-cache-info
                           │     ├─ git fetch origin <repoRef>
                           │     ├─ git checkout --detach FETCH_HEAD
                           │     ├─ append ./hitechos/os/installation.nix to target config
                           │     └─ NIX_PATH=<pinned nixpkgs tarball> \
                           │        nixos-install --root "$ROOT" \
                           │          --option substituters "<cache> https://cache.nixos.org" \
                           │          --option trusted-public-keys "<cache-key> cache.nixos.org-1:..."
                           ├─ exec: users, umount
                           └─ show: finished
```

The stock `nixos` job performs partitioning, mounting and writes
`/etc/nixos/{configuration.nix,hardware-configuration.nix}`. The custom
`shellprocess@hitechos` job runs in the **live** environment (`dontChroot:
true`), so `git`, `curl` and `nixos-install` remain in the ISO and never have to
exist in the freshly unpacked target. This is the Phase 7 bug fix kept as a
hard invariant.

### 4.3 ISO contents and size controls

`os/netinstaller.nix` intentionally overrides the stock installer base:

| Setting | Value | Why |
|---|---|---|
| `system.installer.channel.enable` | `false` | Do not embed the full Nixpkgs source tree (hundreds of MB). |
| `system.extraDependencies` | `[]` | Drop the stock `stdenvNoCC`/busybox/jq payload. |
| `hardware.enableAllHardware` | `false` | Drop all-firmware and all-hardware initrd modules. |
| `hardware.enableRedistributableFirmware` | `false` | No all-firmware payload; a targeted amdgpu subset is added via `hardware.firmware`. |
| `hardware.firmware` | amdgpu subset | RX 580/Polaris firmware, 90 MiB instead of the 1.9 GiB `linux-firmware`. |
| `boot.swraid.enable` | `false` | No software RAID on the live medium. |
| `services.openssh.enable` | `false` | No SSH server on the live medium. |
| `boot.supportedFilesystems.zfs` | `false` | No ZFS module on the live medium. |
| `environment.systemPackages` | merged base + live tools | Do **not** use `lib.mkForce`; the base login/repair packages must stay. |
| `environment.defaultPackages` | installation-cd-base value (`rsync`) | Not forced empty; `bashInteractive`/`coreutils` are explicit packages. |
| `documentation.*` | `false` | No manuals on the medium. |
| `i18n.supportedLocales` | `en_US`, `fr_FR` | All locales are expensive. |
| `isoImage.squashfsCompression` | `xz -Xdict-size 100%` | Better ratio than zstd-19 for this payload. |

The live package set (merged with the base profile) is:

```
bashInteractive, coreutils, util-linux, iproute2, networkmanager, parted,
gptfdisk, dosfstools, e2fsprogs, btrfs-progs, nixos-install-tools, git, curl,
foot, mesa, libglvnd, hitech-installer, sudo, nix, efibootmgr, efivar, kmod,
hwdata, pciutils, usbutils
```

`NIX_PATH` is exported by the installer script to the exact Nixpkgs revision of
the ISO:

```
https://github.com/NixOS/nixpkgs/archive/<config.system.nixos.revision>.tar.gz
```

This keeps the installed system reproducible without shipping the channel.

### 4.4 Size budget — measured vs target

> Historical (Calamares-era) measurement. The bootable 8.1 Rust GUI ISO is
> **823 MiB** (base login/repair toolkit + amdgpu firmware); see §0. The
> Calamares-era options below are retained only for context.

Measured with the Calamares scaffold:

| Item | Value |
|---|---|
| NetInstaller ISO (compressed) | **977 MiB** (500,224 sectors × 2048) |
| Full legacy ISO for comparison | ~1.9 GiB |
| Calamares closure (`calamares-nixos`) | ~2.0 GiB uncompressed |
| NetInstaller system toplevel closure | ~3.5 GiB uncompressed |

The squashfs is dominated by Calamares + Qt 6 + KDE Frameworks
(`ktextwidgets`, `qtspeech`, `qtmultimedia`, `libplasma`, `kio`, `kwallet`,
`gst-plugins-bad`, …). Those are transitive dependencies of the stock
`calamares-nixos` package; removing individual packages at the NixOS level is
not enough.

Options to close the remaining ~180 MiB gap:

1. **Accept ~1.0 GiB** and keep stock Calamares. Lowest risk; the ISO is still
   ~2× smaller than the legacy image.
2. **Custom Calamares build** that strips multimedia/speech/gstreamer and
   unused KDE modules, then validates partitioning and user pages on a VM.
   Risk: upstream package breakage; needs a dedicated nixpkgs override.
3. **Alternative installer frontend** (e.g. a small Rust/GTK or TUI installer)
   if the 800 MiB ceiling is a hard product requirement. This abandons
   Calamares, which is currently a requirement.
4. **Network boot / netboot.xyz-style image** that is only the kernel +
   initrd + installer payload; even smaller but changes the UX and requires
   PXE/HTTP boot infrastructure.

Recommendation for Phase 8: ship option 1, open option 2 as a follow-up with a
VM validation gate, and keep option 3/4 as product decisions rather than
engineering defaults.

### 4.5 Security properties

- The ISO contains **no HiTech-OS secrets**.
- `cachePublicKey` is embedded; if empty, the installer logs a warning and
  signature verification is not enforced. Release images MUST set it.
- `allowSourceFallback = true` is the scaffold default; production images
  should set `false` so a down/compromised cache cannot silently trigger a
  source build of unvetted inputs.
- The only live credential is the ephemeral `nixos`/`hitechos` account. The
  wheel-polkit rule is scoped to the live environment and must never be copied
  to the target.
- `repoRef` is the ISO's own revision; a release ISO is auditable against the
  exact commit that built it.

---

## 5. HiTech-Store binary cache

### 5.1 Why Harmonia

- Rust implementation; low overhead and predictable memory use.
- Multi-worker and socket-activated.
- Native signing-key support (`services.harmonia.cache.signKeyPaths`).
- `nix-serve` remains a documented fallback for existing hosts, but it does not
  sign and is single-process.

`os/services/hitech-cache.nix` enables Harmonia, optionally fronts it with
nginx + ACME, and exposes a client option that sets `substituters` and
`trusted-public-keys`.

### 5.2 Key management

Generate the key pair offline:

```console
nix-store --generate-binary-cache-key hitechos-cache-1 \
  /secure/hitechos-cache-1.secret /secure/hitechos-cache-1.public
```

- Secret key: `/run/secrets/hitechos-cache-signing-key` on the cache host,
  provisioned by sops/agenix. Never commit it.
- Public key: `services.hitech-cache.publicKey`; commit only the public value
  and embed it in the NetInstaller.

### 5.3 Publishing closures (implemented)

Harmonia is read-only, so the pipeline is responsible for building closures,
copying them to the cache host, and signing them where the private key lives.

Two artifacts implement the mechanism:

- **`scripts/publish-cache.sh`** — builds targets, signs locally when a
  trusted builder has the key, copies closures to `ssh-ng://<host>`, and
  optionally has the cache host sign them.
- **`scripts/sign-cache-closure.sh`** — runs on the cache host; signs a store
  path and its entire closure.
- **`.github/workflows/cache-publish.yml`** — `workflow_dispatch` (and
  `cache-*` tags) job that builds `patient-zero` + `hitech-shell`, pushes them
  with a restricted SSH key, and lets the cache host sign the closures. The
  signing key is never a GitHub secret.

Steps on a trusted builder / cache host:

```console
# patient-zero + hitech-shell by default
scripts/publish-cache.sh --sign-key /secure/hitechos-cache-1.secret

# CI path: push unsigned, sign on the cache host
HITECH_CACHE_HOST=cache-publisher@cache.hitechos.tn \
  scripts/publish-cache.sh
```

Build-only and dry-run switches (`--no-push`, `--dry-run`) make the script safe
to run on a developer machine. Every path in a closure is signed
(`nix path-info --recursive "$out" | nix store sign --stdin`) because Nix verifies each
substituted path, not only the toplevel.

Cache-host prerequisites (the deploy user receives unsigned paths):

```nix
users.users.cache-publisher = {
  isNormalUser = true;
  openssh.authorizedKeys.keys = [ "<restricted deploy public key>" ];
};

nix.settings.trusted-users = [ "root" "cache-publisher" ];
```

The deploy user must be able to read the signing key when the host-side signing
step runs (run that step as root, or grant the `cache-publisher` group read
access to `/run/secrets/hitechos-cache-signing-key`). Host-side signing also
works if Harmonia's `signKeyPaths` on-the-fly signing is later disabled; when
both are enabled the operation is idempotent.

Verify from a client after publishing:

```console
nix build .#nixosConfigurations.patient-zero.config.system.build.toplevel \
  --option substituters "https://cache.hitechos.tn https://cache.nixos.org" \
  --option trusted-public-keys "hitechos-cache-1:... cache.nixos.org-1:..."
```

Then record the cache path and NAR hash in the release notes.

Do **not** put the signing key in a GitHub Actions secret: a compromised CI
secret lets an attacker sign arbitrary store paths. The default pipeline keeps
signing on the cache host; a self-hosted runner on the cache host is the
alternative and can call `publish-cache.sh --sign-key` directly.

### 5.4 Client configuration

Server side:

```nix
services.hitech-cache = {
  enable = true;
  domain = "cache.hitechos.tn";
  enableTls = true;
  openFirewall = true;
  signKeyPath = "/run/secrets/hitechos-cache-signing-key";
  publicKey = "hitechos-cache-1:<public>";
};
```

Client side (patient-zero, installed targets, NetInstaller):

```nix
services.hitech-cache.client = {
  enable = true;
  url = "https://cache.hitechos.tn";
  publicKey = "hitechos-cache-1:<public>";
};
```

The NetInstaller currently configures the same values directly in
`os/netinstaller.nix` (`cacheUrl`, `cachePublicKey`) because the live ISO does
not import the server module.

### 5.5 Store integration

HiTech-Store (`services.hitech-store`) and the cache (`services.hitech-cache`)
are separate systemd services on the same host. The store stays the
application/portal; Harmonia stays the content-addressed store. A future
iteration can expose a store admin panel for cache status and key rotation,
but the security boundary must remain: the store does not receive the signing
key.

---

## 6. HiTech-Shell (Rust Wayland DE)

### 6.1 Scope of the scaffold

`apps/hitech-shell` currently contains:

- `Cargo.toml` / `Cargo.lock` — a dependency-free Rust 2021 binary.
- `src/main.rs` — the entry point and NixOS contract smoke test.
- `src/config.rs` — `Layout` enum (`hybrid`, `mac`, `windows`) and `Config`.
- `src/layout/mod.rs` — pure layout-region table (anchor, layer, height).
- `src/compositor.rs` — the Smithay seam (event loop placeholder).
- `src/shell/mod.rs` — the shell-chrome seam (Slint/Iced placeholder).
- `default.nix` — direct `rustc` build + Wayland session file.

Why no Smithay/Slint dependency yet: the scaffold must build on the cache
builder and on CI without vendoring a large registry. The traits/enums above
are the stable seams; the next iteration adds dependencies behind feature
flags (`smithay`, `slint-ui`, `iced-ui`) and switches `default.nix` to
`rustPlatform.buildRustPackage`.

### 6.2 Process architecture (target)

```
┌──────────────────────────────┐
│ hitech-shell (compositor)    │  Smithay + calloop
│  - DRM/KMS backend           │
│  - wlr-layer-shell           │
│  - XWayland                  │
│  - input/output management   │
└──────────────┬───────────────┘
               │ Wayland / IPC
┌──────────────┴───────────────┐
│ hitech-shell-ui (chrome)     │  Slint layer-shell client
│  - taskbar / dock            │
│  - start menu / control ctr  │
│  - quick settings            │
│  - notifications             │
└──────────────────────────────┘
               │
┌──────────────┴───────────────┐
│ hitech-shell-settings        │  optional Iced app
└──────────────────────────────┘
```

Keeping chrome in a separate client means a compositor crash does not
necessarily take the panel down, and the UI toolkit is replaceable.

### 6.3 NixOS contract

`os/services/hitech-shell.nix`:

```nix
services.hitech-shell = {
  enable = true;
  layout = "hybrid"; # "hybrid" | "mac" | "windows"
  outputs = [ "*" ]; # per-monitor policies later
  session.enable = true; # register with services.displayManager.sessionPackages
  extraConfig = { theme = "hitechos-dark"; };
};
```

Generated config:

```
/etc/hitechos/hitech-shell.toml
```

Exported session variables:

```
HITECH_SHELL_CONFIG=/etc/hitechos/hitech-shell.toml
HITECH_SHELL_LAYOUT=hybrid
HITECH_SHELL_OUTPUTS=*
```

The Rust `Config::from_env()` reads the same values, so the NixOS option is the
single source of truth.

### 6.4 Layout profiles

| Region | `hybrid` | `mac` | `windows` |
|---|---|---|---|
| Bottom bar | Windows-style taskbar | macOS-style dock | Windows-style taskbar |
| Top bar | — | macOS-style menu bar | — |
| Start menu | Windows-style | — | Windows-style |
| Quick settings | Android-style right panel | macOS-style control centre | System tray |
| Notifications | Top overlay | Top overlay | Top overlay |
| Workspace switcher | Hybrid (taskbar + gestures) | Dock/gestures | Taskbar |

The region table is pure data (`src/layout/mod.rs`), so the same profiles can
be tested without a running compositor.

---

## 7. Integration map

```
flake.nix
 ├─ nixosConfigurations.patient-zero   → os/configuration.nix → os/installation.nix
 │                                                              ├─ common/ecosystem/store/genio/secrets
 │                                                              ├─ services/hitech-cache.nix  (client optional)
 │                                                              └─ services/hitech-shell.nix  (disabled by default)
 ├─ nixosConfigurations.netinstaller   → installation-cd-base + os/netinstaller.nix
 │                                        ├─ cage + Calamares
 │                                        ├─ pinned Nixpkgs tarball (NIX_PATH)
 │                                        └─ HiTech-Store cache substituter
 ├─ nixosConfigurations.iso            → legacy full graphical image
 └─ packages.{default,hitech-shell}    → apps/hitech-shell
```

Cache host (future dedicated profile or patient-zero with the options enabled):

```
services.hitech-store.enable = true;   # portal
services.hitech-cache.enable = true;   # Harmonia cache
```

---

## 8. Rollout plan

| Step | Deliverable | Exit criteria |
|---|---|---|
| 8.0 | Architecture + scaffold (this document) | `nix flake check --no-build` passes; NetInstaller and `hitech-shell` build |
| 8.1 | Cache deployment + key generation | `nix-cache-info` reachable over HTTPS; client trust verified; one signed closure fetched |
| 8.2 | Cache publish pipeline (scripts + `cache-publish.yml`) | `patient-zero` closure appears in cache after a manual run; no private key in CI; client verifies signature |
| 8.3 | NetInstaller VM validation | Full install in QEMU/UEFI from ISO; target boots; services start |
| 8.4 | Physical-hardware validation | One real install reaches the cache path; rollback path documented |
| 8.5 | HiTech-Shell compositor v0 | Smithay DRM backend starts on real hardware; layer-shell regions from `layout/mod.rs` |
| 8.6 | HiTech-Shell chrome v0 | Slint panel/dock/quick-settings for all three profiles |
| 8.7 | Size decision | Either accept ~1.0 GiB or implement the Calamares slim-down plan |

### 8.1 Validation notes recorded so far

- `nix flake check --no-build` passes for `devShells`, `packages`,
  `nixosConfigurations.netinstaller`, `patient-zero`, and `iso`.
- `nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage`
  succeeds; ISO size 823 MiB after the 8.1 Rust GUI replacement and the
  login/serial fix (the older 977 MiB figure in §4.4 is the Calamares-era
  measurement).
- `nix build .#hitech-shell` succeeds; `nix run .#hitech-shell` prints the
  hybrid layout plan and compositor placeholder.
- `patient-zero` toplevel derivation evaluates successfully with the new
  `hitech-cache` and `hitech-shell` modules (both disabled by default).
- Cache publish mechanism validated locally: `sign-cache-closure.sh` signed a
  7-path closure, `nix copy` propagated the signature into a second store, and
  `nix store verify --recursive --sigs-needed 1` accepted the copy.
- QEMU boot validated with `-vga virtio`: serial autologin, `/run/current-system/sw/bin/bash`,
  `cage-tty1` active, and the Rust Welcome screen rendered. Full install, cache
  server runtime test, `ssh-ng` push against the real host, and physical
  install are still pending.

---

## 9. Test plan

### NetInstaller

```console
nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage
```

Then in a VM:

1. Boot the ISO UEFI.
2. Confirm `cage` starts and Calamares opens.
3. Install to a blank disk with the cache reachable.
4. Confirm the target boots and `systemctl status hitechos-secrets
   hitech-store genio` is healthy.
5. Re-run with the cache blocked and `allowSourceFallback=false`; the installer
   must fail closed with a clear log.

### Binary cache

```console
curl -fsS https://cache.hitechos.tn/nix-cache-info
nix build .#nixosConfigurations.patient-zero.config.system.build.toplevel \
  --option substituters "https://cache.hitechos.tn https://cache.nixos.org" \
  --option trusted-public-keys "hitechos-cache-1:... cache.nixos.org-1:..."
nix path-info --json -r --closure-size result | jq 'length'
```

Also test a tampered/unsigned path: with `require-sigs = true`, Nix must refuse
it.

### HiTech-Shell

```console
nix build .#hitech-shell
nix run .#hitech-shell
HITECH_SHELL_LAYOUT=mac nix run .#hitech-shell
HITECH_SHELL_LAYOUT=windows nix run .#hitech-shell
```

Once Smithay lands: nested compositor test (winit backend) and DRM test on real
hardware.

---

## 10. Risks and open decisions

| Risk | Impact | Mitigation |
|---|---|---|
| ~1.0 GiB NetInstaller | Missed ~800 MiB product target | Accept, or slim Calamares with a VM-gated override; document decision |
| Calamares partitioning without `programs.partition-manager` | Partition page may fail | VM test; if it fails, add the smallest required kpmcore/partition tools back |
| Wired-only live network | Wi-Fi laptops cannot install | Add a firmware/iwlwifi package only if product requires Wi-Fi |
| Cache private key in CI | Arbitrary signed paths | Implemented: CI copies unsigned paths over a restricted SSH key; cache host signs; self-hosted runner can sign directly |
| `require-sigs` and empty `cachePublicKey` | Unsigned cache rejected or silently unverified | Release builds must set `cachePublicKey` and `allowSourceFallback=false` |
| Source fallback rebuild time | Installer appears hung | Cache health check and clear log messages already in the script |
| Rust DE scope | Months of work | Keep compositor/UI seams separate; ship headless compositor first |
| No physical validation | Unknown installer bugs | VM gate first; physical test before release |

Open decisions needing the owner:

1. Is ~1.0 GiB acceptable, or is 800 MiB a hard requirement?
2. Is Wi-Fi required on the live medium, or is wired Ethernet enough?
3. Cache hostname/TLS certificate ownership (`cache.hitechos.tn` is a
   placeholder).
4. ~~Cache publish mechanism: self-hosted runner vs restricted SSH push.~~
   Implemented as restricted SSH push with host-side signing; self-hosted
   runner remains supported via `publish-cache.sh --sign-key`.
5. Shell UI toolkit: Slint first (as recommended) or Iced first.

---

## 11. Appendix — commands used to verify the scaffold

```console
nix flake check --no-build
nix build .#hitech-shell
nix run .#hitech-shell
nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage
nix eval --raw .#nixosConfigurations.netinstaller.config.image.fileName
nix eval .#nixosConfigurations.netinstaller.config.nix.settings --json
nix eval --raw .#nixosConfigurations.patient-zero.config.system.build.toplevel.drvPath
```

Measured ISO size:

```console
OUT=$(nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage \
  --no-link --print-out-paths)
ls -lh "$OUT/iso/"*.iso
```

Cache publish mechanism (local end-to-end, no cache host required):

```console
nix key generate-secret --key-name hitechos-cache-test > /tmp/test.secret
nix key convert-secret-to-public < /tmp/test.secret > /tmp/test.public
PATHOUT=$(nix build .#hitech-shell --no-link --print-out-paths)
scripts/sign-cache-closure.sh -k /tmp/test.secret "$PATHOUT"
nix copy --to "local?root=/tmp/dummy-store" --no-check-sigs "$PATHOUT"
nix store verify --recursive --sigs-needed 1 \
  --option trusted-public-keys "$(cat /tmp/test.public)" "$PATHOUT"
```

Argument/dry-run checks:

```console
scripts/publish-cache.sh --help
scripts/publish-cache.sh --dry-run hitech-shell
scripts/publish-cache.sh --no-push --sign-key /tmp/test.secret hitech-shell
```

---

## 12. Files added/changed in Phase 8

- `flake.nix` — added `packages.{default,hitech-shell}` and
  `nixosConfigurations.netinstaller`.
- `os/netinstaller.nix` — minimal Calamares ISO, pinned Nixpkgs tarball,
  cache substituter, size controls.
- `os/services/hitech-cache.nix` — Harmonia cache server + client options.
- `os/services/hitech-shell.nix` — Rust shell NixOS contract.
- `os/installation.nix` — imports the two new service modules.
- `apps/hitech-shell/` — Cargo project, Rust source, Nix derivation, Wayland
  session file.
- `scripts/publish-cache.sh` — build/sign/push orchestration for the cache.
- `scripts/sign-cache-closure.sh` — host-side closure signer.
- `.github/workflows/cache-publish.yml` — restricted-SSH publish job with
  host-side signing (no private key in CI).
- `docs/ROADMAP.md` — Phase 8 architecture section and status update.
- `docs/audit/PHASE8-NETINSTALLER-RUST-DE.md` — this document.
