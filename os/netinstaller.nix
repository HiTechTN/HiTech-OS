# HiTech-OS — Phase 8 NetInstaller (minimal Calamares ISO).
#
# Goal: a small (~800 MB target) live ISO that contains only:
#   - a Wayland kiosk running Calamares,
#   - networking tools,
#   - git + curl,
#   - the NixOS installer machinery.
#
# The actual HiTech-OS closure is NOT in the ISO. Calamares:
#   1. partitions and mounts the target (stock `nixos` job),
#   2. runs `shellprocess@hitechos`, which fetches the HiTech-OS config pinned
#      to the ISO's revision from GitHub,
#   3. calls `nixos-install` with the HiTech-Store binary cache as the primary
#      substituter, so the target closure is downloaded instead of rebuilt.
#
# Two outputs are therefore produced by the flake:
#   nixosConfigurations.netinstaller       — this minimal ISO
#   nixosConfigurations.iso                — legacy full graphical ISO
#
# Security contract:
#   - `cachePublicKey` is embedded in the ISO; the cache is trusted only if
#     its signature verifies. Leave it empty in development only.
#   - `allowSourceFallback = false` is the production default once the cache
#     has a stable release; it prevents a compromised/down cache from silently
#     rebuilding unvetted source on the target.

{ config, lib, pkgs, hitechosRevision ? "main", ... }:

let
  cfg = config.hitechos.netinstaller;

  # Pull the HiTech-Store/Harmonia cache public key from the option. The NixOS
  # module for the cache server exposes the same value as a read-only option
  # when co-located; this stays a plain string in the ISO so the installer has
  # no dependency on the server module.
  cachePublicKey = cfg.cachePublicKey;

  # Channel-free installer: the ISO does not embed the full Nixpkgs source
  # tree (that alone is hundreds of MB). Instead the live script points
  # NIX_PATH at the exact Nixpkgs revision baked into the ISO; nixos-install
  # fetches it over the network, which the NetInstaller requires anyway for the
  # closure cache. The revision is the same one used to build the ISO, so the
  # installed system stays reproducible.
  nixpkgsArchive =
    if config.system.nixos.revision != null then
      "https://github.com/NixOS/nixpkgs/archive/${config.system.nixos.revision}.tar.gz"
    else
      "https://github.com/NixOS/nixpkgs/archive/nixos-unstable.tar.gz";

  hitechosPostInstall = pkgs.writeShellScript "hitechos-netinstaller" ''
    set -eu

    log() {
      printf '[hitechos-netinstaller] %s\n' "$*"
    }

    target="$1"
    repo_url=${lib.escapeShellArg cfg.repoUrl}
    repo_ref=${lib.escapeShellArg cfg.repoRef}
    cache_url=${lib.escapeShellArg cfg.cacheUrl}
    cache_key=${lib.escapeShellArg cachePublicKey}
    allow_source_fallback=${lib.boolToString cfg.allowSourceFallback}

    repo_rel="/etc/nixos/hitech-os"
    repo="$target$repo_rel"
    cfg_file="$target/etc/nixos/configuration.nix"
    import_rel="./hitech-os/os/installation.nix"

    [ -f "$cfg_file" ] || {
      log "configuration.nix not found at $cfg_file; did the stock nixos job run?"
      exit 1
    }

    if ${pkgs.curl}/bin/curl -fsS --max-time 20 "$cache_url/nix-cache-info" >/dev/null 2>&1; then
      log "binary cache reachable: $cache_url"
    else
      log "WARNING: binary cache unreachable: $cache_url"
      if [ "$allow_source_fallback" != "true" ]; then
        log "allowSourceFallback=false: aborting instead of rebuilding from source"
        exit 1
      fi
      log "allowSourceFallback=true: continuing; target closure will be built locally"
    fi

    # Fetch exactly the revision baked into the ISO. When repoRef is a commit
    # SHA, GitHub serves it through uploadpack.allowReachableSHA1InWant; when
    # it is a branch/tag, this resolves to that ref. In production the flake
    # passes `self.rev`, so a clean release ISO is pinnable.
    if [ ! -d "$repo/.git" ]; then
      mkdir -p "$repo"
      ${pkgs.git}/bin/git -C "$repo" init -q
      ${pkgs.git}/bin/git -C "$repo" remote add origin "$repo_url" 2>/dev/null || true
    fi
    ${pkgs.git}/bin/git -C "$repo" remote set-url origin "$repo_url"
    ${pkgs.git}/bin/git -C "$repo" fetch --depth 1 origin "$repo_ref"
    ${pkgs.git}/bin/git -C "$repo" checkout --detach --force FETCH_HEAD

    # Keep the target's hardware/bootloader config untouched; only append the
    # HiTech-OS profile import. Idempotent so a retried install does not add
    # the line twice.
    if ! ${pkgs.gnugrep}/bin/grep -qF "$import_rel" "$cfg_file"; then
      ${pkgs.gnused}/bin/sed -i "\|hardware-configuration.nix|a\\      $import_rel" "$cfg_file"
    fi

    trusted_keys="cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    if [ -n "$cache_key" ]; then
      trusted_keys="$cache_key $trusted_keys"
      log "trusting cache key: $cache_key"
    else
      log "WARNING: no cache public key configured; cache signatures are not verified"
    fi

    # The generated target configuration is a plain NixOS config (Calamares
    # generates it), so it resolves `<nixpkgs>` through NIX_PATH. Point it at
    # the exact revision used to build this ISO instead of bundling the whole
    # channel in the image.
    export NIX_PATH="nixpkgs=${nixpkgsArchive}"
    log "NIX_PATH=nixpkgs=${nixpkgsArchive}"
    log "running nixos-install (substituters: $cache_url https://cache.nixos.org)"
    ${pkgs.nixos-install}/bin/nixos-install \
      --root "$target" \
      --no-root-passwd \
      --option substituters "$cache_url https://cache.nixos.org" \
      --option trusted-public-keys "$trusted_keys" \
      --option extra-experimental-features "nix-command flakes"
  '';

  hitechosShellprocessConf = pkgs.writeText "shellprocess-hitechos.conf" ''
    ---
    # Runs in the LIVE environment (dontChroot: true), from which the target is
    # mounted at ''${ROOT}. This keeps git/curl/nixos-install in the ISO and out
    # of the freshly unpacked target.
    dontChroot: true
    timeout: 7200
    script:
      - '${hitechosPostInstall} "''${ROOT}"'
  '';

  # Deliberately small Calamares sequence. It uses the stock NixOS `nixos`
  # job only for partition/mount/base install; the HiTech-OS closure comes
  # from the cache in `shellprocess@hitechos`.
  hitechosSettingsConf = pkgs.writeText "settings.conf" ''
    ---
    modules-search: [ local, @out@/lib/calamares/modules ]

    instances:
    - module:   nixos
      weight:   48
    - id:       hitechos
      module:   shellprocess
      config:   shellprocess-hitechos.conf

    sequence:
    - show:
      - welcome
      - locale
      - keyboard
      - users
      - partition
      - summary
    - exec:
      - partition
      - mount
      - nixos
      - shellprocess@hitechos
      - users
      - umount
    - show:
      - finished

    branding: nixos
    prompt-install: false
    dont-chroot: false
    oem-setup: false
    disable-cancel: false
    disable-cancel-during-exec: false
    hide-back-and-next-during-exec: false
    quit-at-end: false
  '';

  calamaresExtensions = pkgs.calamares-nixos-extensions.overrideAttrs (old: {
    postInstall = (old.postInstall or "") + ''
      cp ${hitechosShellprocessConf} $out/etc/calamares/modules/shellprocess-hitechos.conf
      cp ${hitechosSettingsConf} $out/etc/calamares/settings.conf
      # Our copy reintroduces the raw @out@ placeholder; the package's own
      # postInstall already substituted its settings.conf, so substitute ours too.
      substituteInPlace $out/etc/calamares/settings.conf --replace-fail '@out@' "$out"
    '';
  });

  calamares = pkgs.calamares-nixos.override {
    calamares-nixos-extensions = calamaresExtensions;
  };
in
{
  options.hitechos.netinstaller = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Enable the Phase 8 NetInstaller ISO configuration. This module is only
        imported by `nixosConfigurations.netinstaller`, so it defaults to true;
        set false only to evaluate the bare upstream minimal ISO.
      '';
    };

    repoUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://github.com/HiTechTN/HiTech-OS.git";
      description = "Git remote fetched by Calamares inside the live session.";
    };

    repoRef = lib.mkOption {
      type = lib.types.str;
      default = hitechosRevision;
      defaultText = lib.literalExpression "self.rev or \"main\"";
      description = ''
        Git revision baked into the ISO. In release builds this is the flake's
        `self.rev`, so the installed system is reproducible and auditable.
      '';
    };

    cacheUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://cache.hitechos.local";
      description = "HiTech-Store/Harmonia binary cache base URL.";
    };

    cachePublicKey = lib.mkOption {
      type = lib.types.str;
      default = "hitechos-cache-1:vTmn/dRSOqEFdrpLs13HYOLv1DnYLvYnDfu7icj3dQY=";
      description = ''
        Public key of the binary cache embedded in the ISO. Empty disables
        signature verification; production images MUST set this.
      '';
    };

    nixpkgsArchive = lib.mkOption {
      type = lib.types.str;
      internal = true;
      default = nixpkgsArchive;
      description = ''
        Internal: Nixpkgs tarball URL exported as NIX_PATH by the installer
        script. Derived from the ISO's exact Nixpkgs revision.
      '';
    };

    installerScript = lib.mkOption {
      type = lib.types.path;
      internal = true;
      default = hitechosPostInstall;
      description = ''
        Internal: the `shellprocess@hitechos` script. Exposed for inspection
        and tests; not a user-facing option.
      '';
    };

    allowSourceFallback = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        If true, the installer continues with a source build when the cache is
        unreachable. Set false for production/release images so a cache outage
        fails closed instead of silently rebuilding unvetted inputs.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # --- Base live system -------------------------------------------------
    networking.hostName = "hitechos-netinstaller";
    networking.networkmanager.enable = true;

    nix.settings = {
      experimental-features = [ "nix-command" "flakes" ];
      # Prefer the HiTech cache; cache.nixos.org remains the upstream fallback
      # contributed by the NixOS module defaults.
      substituters = lib.mkBefore [ cfg.cacheUrl ];
      trusted-public-keys = lib.mkAfter (lib.optional (cachePublicKey != "") cachePublicKey);
      connect-timeout = 10;
      fallback = true;
    };

    # --- Graphical installer surface -------------------------------------
    # cage is a ~tiny Wayland kiosk compositor; it keeps Qt/Calamares out of a
    # full GNOME stack and is the main reason this ISO can approach 800 MB.
    services.cage = {
      enable = true;
      user = "nixos";
      program = "${calamares}/bin/calamares";
      environment = {
        QT_QPA_PLATFORM = "wayland";
        QT_WAYLAND_DISABLE_WINDOWDECORATION = "1";
      };
    };

    services.libinput.enable = true;
    security.polkit.enable = true;
    security.polkit.enablePkexecWrapper = true;
    # NOTE: `programs.partition-manager.enable` is intentionally NOT set here.
    # It pulls the full KDE Partition Manager/Qt stack (~1.7 GiB closure) into
    # the ISO. Calamares' partition job uses its own kpmcore library; validate
    # real partitioning before trusting this in production.

    # Live-session policy: wheel (the live `nixos` user) may run pkexec. This
    # is scoped to the ephemeral ISO and must never be copied to the target.
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (subject.isInGroup("wheel")) {
          return polkit.Result.YES;
        }
      });
    '';

    # `profiles/base.nix` from installation-cd-base ships a large repair
    # toolkit (testdisk, ddrescue, w3m, ms-sys, …). Force a curated package
    # set instead: Calamares + its partitioning/user tools + git/curl + Nix.
    environment.systemPackages = lib.mkForce (with pkgs; [
      calamares
      git
      curl
      nixos-install
      nix
      gnugrep
      gnused
      networkmanager
      util-linux
      parted
      gptfdisk
      e2fsprogs
      dosfstools
      btrfs-progs
      xfsprogs
      ntfs3g
      lvm2
      cryptsetup
      efibootmgr
      efivar
      kmod
      hwdata
      pciutils
      usbutils
    ]);

    # --- Deliberate size reductions --------------------------------------
    # The stock installation-cd base is built for every possible machine:
    # full Nixpkgs channel, all firmware, ZFS, software RAID, OpenSSH. The
    # NetInstaller needs none of that to reach the cache; the closures it
    # installs come from HiTech-Store. Wired Ethernet (and a curated set of
    # initrd storage/USB modules) is the supported live networking path.
    system.installer.channel.enable = false;
    system.extraDependencies = lib.mkForce [ ];
    hardware.enableAllHardware = lib.mkForce false;
    hardware.enableRedistributableFirmware = lib.mkForce false;
    boot.swraid.enable = lib.mkForce false;
    services.openssh.enable = lib.mkForce false;
    boot.supportedFilesystems.zfs = lib.mkForce false;

    # Minimal initrd module set: enough for IDE/SATA/NVMe/USB/virtio disks
    # and the squashfs live root, without pulling all-hardware firmware.
    boot.initrd.availableKernelModules = [
      "ahci"
      "ata_piix"
      "nvme"
      "uas"
      "usb_storage"
      "xhci_pci"
      "ehci_pci"
      "ohci_pci"
      "usbhid"
      "hid_generic"
      "sdhci"
      "sdhci_pci"
      "virtio_blk"
      "virtio_pci"
      "virtio_scsi"
      "virtio_net"
      "ext4"
      "vfat"
      "squashfs"
      "overlay"
      "loop"
    ];

    # Equivalent of profiles/minimal.nix; we do not import that profile directly
    # because installation-cd-base already imports profiles/base.nix.
    environment.defaultPackages = lib.mkForce [ ];
    programs.command-not-found.enable = lib.mkForce false;
    xdg.autostart.enable = lib.mkForce false;
    xdg.icons.enable = lib.mkForce false;
    xdg.mime.enable = lib.mkForce false;
    xdg.sounds.enable = lib.mkForce false;
    services.logrotate.enable = lib.mkForce false;
    services.udisks2.enable = lib.mkForce false;

    documentation.enable = lib.mkForce false;
    documentation.man.enable = lib.mkForce false;
    documentation.nixos.enable = lib.mkForce false;
    i18n.supportedLocales = [
      "en_US.UTF-8/UTF-8"
      "fr_FR.UTF-8/UTF-8"
    ];
    i18n.defaultLocale = "en_US.UTF-8";
    console.keyMap = "us";
    fonts.packages = [ pkgs.dejavu_fonts ];

    # --- Live credentials (ephemeral; never reused on the target) ---------
    users.users.nixos.initialHashedPassword = lib.mkForce null;
    users.users.nixos.initialPassword = "hitechos";
    users.users.nixos.extraGroups = [ "wheel" "networkmanager" ];

    # Calamares/KDE dominates the squashfs payload. xz compresses the store
    # noticeably better than zstd-19 for this workload at the cost of a slower
    # image build; the compressed ISO is the artifact users download.
    isoImage.squashfsCompression = lib.mkDefault "xz -Xdict-size 100%";

    # --- Boot media identity ---------------------------------------------
    isoImage.volumeID = "HITECHOS_NET";
    isoImage.edition = "netinstaller";
    image.fileName = lib.mkForce
      "hitechos-netinstaller-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso";
  };
}
