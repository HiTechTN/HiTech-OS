# Phase 8 — NetInstaller minimal + HiTech-Shell (Rust Wayland DE)

Status: architecture + scaffold landed; runtime implementation and cache upload pipeline are next.
Date: 2026-10-23
Repo: `github:HiTechTN/HiTech-OS`
Base commit: `2053f58` (Phase 7) + Phase 8 working tree

This document is the master reference for the Phase 8 pivot. It is deliberately
written so that a future agent can resume without re-reading the whole repo.

---

## 1. Executive summary

Phase 7 made the legacy full graphical ISO (GNOME + Flatpak + Firefox +
Calamares, ~1.9 GiB measured) install HiTech-OS services correctly. Phase 8
replaces that image with two separate, smaller artefacts:

1. **NetInstaller** — a minimal Calamares ISO (`nixosConfigurations.netinstaller`)
   that only contains the installer, networking tools, `git`/`curl`, and the
   NixOS installation machinery. It clones the HiTech-OS configuration from
   GitHub at the revision baked into the ISO and installs the target closure
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
| NetInstaller ISO | `nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage` | built; **977 MiB** (target ~800 MiB — see §4.4) |
| HiTech-Shell scaffold | `nix build .#hitech-shell` / `nix run .#hitech-shell` | built and runs (`hybrid` layout by default) |
| Flake evaluation | `nix flake check --no-build` | all checks passed |
| Legacy full ISO | unchanged | still available as `.#nixosConfigurations.iso` |

The ~800 MiB target is **not yet met**. The remaining gap is Calamares/Qt/KDE
runtime, which dominates the compressed squashfs payload. §4.4 lists the
reduction options and the trade-offs; it is an explicit open decision, not a
silent regression.

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
| `hardware.enableRedistributableFirmware` | `false` | Same; wired-only live networking is the trade-off. |
| `boot.swraid.enable` | `false` | No software RAID on the live medium. |
| `services.openssh.enable` | `false` | No SSH server on the live medium. |
| `boot.supportedFilesystems.zfs` | `false` | No ZFS module on the live medium. |
| `environment.systemPackages` | curated list | Replaces the `profiles/base.nix` repair toolkit. |
| `environment.defaultPackages` | `[]` | Equivalent of `profiles/minimal.nix` (Perl etc.). |
| `documentation.*` | `false` | No manuals on the medium. |
| `i18n.supportedLocales` | `en_US`, `fr_FR` | All locales are expensive. |
| `isoImage.squashfsCompression` | `xz -Xdict-size 100%` | Better ratio than zstd-19 for this payload. |

The curated package list is:

```
calamares, git, curl, nixos-install, nix, gnugrep, gnused, networkmanager,
util-linux, parted, gptfdisk, e2fsprogs, dosfstools, btrfs-progs, xfsprogs,
ntfs3g, lvm2, cryptsetup, efibootmgr, efivar, kmod, hwdata, pciutils, usbutils
```

`NIX_PATH` is exported by the installer script to the exact Nixpkgs revision of
the ISO:

```
https://github.com/NixOS/nixpkgs/archive/<config.system.nixos.revision>.tar.gz
```

This keeps the installed system reproducible without shipping the channel.

### 4.4 Size budget — measured vs target

Measured with the current scaffold:

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

### 5.3 Publishing closures

Harmonia is read-only. The recommended pipeline is:

1. Build the closure on a trusted builder:
   `nix build .#nixosConfigurations.patient-zero.config.system.build.toplevel`
2. Push it to the cache host without exposing the private key to CI:
   - preferred: a self-hosted runner on the cache host, or
   - `nix copy --to ssh-ng://cache-host` from a restricted CI key, then sign on
     the cache host with `nix store sign --key-file /run/secrets/...`.
3. Verify from a client:
   ```console
   nix build .#nixosConfigurations.patient-zero.config.system.build.toplevel \
     --option substituters "https://cache.hitechos.tn https://cache.nixos.org" \
     --option trusted-public-keys "hitechos-cache-1:... cache.nixos.org-1:..."
   ```
4. Record the cache path and NAR hash in the release notes.

Do **not** put the signing key in a GitHub Actions secret unless a self-hosted
runner is impossible; a compromised CI secret lets an attacker sign arbitrary
store paths.

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
| 8.2 | CI publish job | `patient-zero` closure appears in cache after a `main` build; no private key in CI |
| 8.3 | NetInstaller VM validation | Full install in QEMU/UEFI from ISO; target boots; services start |
| 8.4 | Physical-hardware validation | One real install reaches the cache path; rollback path documented |
| 8.5 | HiTech-Shell compositor v0 | Smithay DRM backend starts on real hardware; layer-shell regions from `layout/mod.rs` |
| 8.6 | HiTech-Shell chrome v0 | Slint panel/dock/quick-settings for all three profiles |
| 8.7 | Size decision | Either accept ~1.0 GiB or implement the Calamares slim-down plan |

### 8.1 Validation notes recorded so far

- `nix flake check --no-build` passes for `devShells`, `packages`,
  `nixosConfigurations.netinstaller`, `patient-zero`, and `iso`.
- `nix build .#nixosConfigurations.netinstaller.config.system.build.isoImage`
  succeeds; ISO size 977 MiB.
- `nix build .#hitech-shell` succeeds; `nix run .#hitech-shell` prints the
  hybrid layout plan and compositor placeholder.
- `patient-zero` toplevel derivation evaluates successfully with the new
  `hitech-cache` and `hitech-shell` modules (both disabled by default).
- Not yet done: ISO boot in a VM, Calamares run, cache server runtime test,
  signing/publishing a real closure, physical install.

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
| Cache private key in CI | Arbitrary signed paths | Self-hosted runner / sign on cache host; never CI secret by default |
| `require-sigs` and empty `cachePublicKey` | Unsigned cache rejected or silently unverified | Release builds must set `cachePublicKey` and `allowSourceFallback=false` |
| Source fallback rebuild time | Installer appears hung | Cache health check and clear log messages already in the script |
| Rust DE scope | Months of work | Keep compositor/UI seams separate; ship headless compositor first |
| No physical validation | Unknown installer bugs | VM gate first; physical test before release |

Open decisions needing the owner:

1. Is ~1.0 GiB acceptable, or is 800 MiB a hard requirement?
2. Is Wi-Fi required on the live medium, or is wired Ethernet enough?
3. Cache hostname/TLS certificate ownership (`cache.hitechos.tn` is a
   placeholder).
4. Cache publish mechanism: self-hosted runner vs restricted SSH push.
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
- `docs/ROADMAP.md` — Phase 8 architecture section and status update.
- `docs/audit/PHASE8-NETINSTALLER-RUST-DE.md` — this document.
