# HiTech-OS — Phase 8 NetInstaller (minimal eframe/egui ISO).
#
# Goal: a small (~800 MB target) live ISO that contains only:
#   - a Wayland kiosk running the Rust `hitech-installer` GUI,
#   - networking tools,
#   - git + curl,
#   - the NixOS installer machinery.
#
# The actual HiTech-OS closure is NOT in the ISO. The graphical installer:
#   1. partitions/formats/mounts the target,
#   2. fetches the HiTech-OS profile pinned to the ISO's revision from GitHub,
#   3. writes the operator's anonymous identity into configuration.nix,
#   4. calls `nixos-install` with the HiTech-Store binary cache as the primary
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

  # Graphical HiTech-OS installer (eframe/egui). It replaces Calamares and
  # owns the partition/format/mount/install pipeline.
  hitechInstaller = pkgs.callPackage ../hitech-installer { };

  # Wrapper script that detects missing hardware acceleration and forces
  # software rendering (llvmpipe) so the eframe/glow HUD appears instead of
  # a black screen on VirtualBox/QEMU-without-virgl/headless CI.
  #
  # Detection strategy:
  #  1. If NO DRM render node exists at all (not just renderD0), force
  #     software rendering for both GL and Wayland/Ozone.
  #  2. Check for common virtual-machine indicators (QEMU virtio-gpu, VirtualBox
  #     VBoxGuest, VMware vmware) so llvmpipe is used even when a stub DRM
  #     device is present but hardware acceleration is not usable.
  hitechInstallerKiosk = pkgs.writeScriptBin "hitech-installer-kiosk" ''
    soft=0
    if [ ! -e /dev/dri/renderD0 ] && [ ! -e /dev/dri/card0 ]; then
      soft=1
    fi
    if [ -e /sys/module/virtio_gpu ] || [ -e /sys/module/vboxguest ] || grep -aq vmware /proc/1/environ 2>/dev/null; then
      soft=1
    fi
    if [ "$soft" = "1" ]; then
      export LIBGL_ALWAYS_SOFTWARE=1
      export WLR_RENDERER_ALLOW_SOFTWARE=1
      export WLR_RENDERER=soft
      export EGL_PLATFORM=x11
      export GST_VAAPI_ALL_DRIVERS=1
    fi
    exec ${hitechInstaller}/bin/hitech-installer "$@"
  '';

in
{
  imports = [ ./hardware-universal.nix ];

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
      description = "Git remote fetched by the graphical installer inside the live session.";
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
        Internal: Nixpkgs tarball URL exported as NIX_PATH by the graphical
        installer. Derived from the ISO's exact Nixpkgs revision.
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
    # cage is a tiny Wayland kiosk compositor. The eframe/egui installer owns
    # the whole installation flow, so Calamares/Qt/KDE are no longer pulled in.
    services.cage = {
      enable = true;
      user = "nixos";
      program = "${hitechInstallerKiosk}/bin/hitech-installer-kiosk";
      environment = {
        # Consumed by the Rust installer at install time.
        HITECH_CACHE_URL = cfg.cacheUrl;
        HITECH_CACHE_KEY = cachePublicKey;
        HITECH_NIXPKGS_ARCHIVE = nixpkgsArchive;
        HITECH_STATE_VERSION = config.system.stateVersion;
        HITECH_REPO_URL = cfg.repoUrl;
        HITECH_REPO_REF = cfg.repoRef;
        HITECH_ALLOW_SOURCE_FALLBACK = lib.boolToString cfg.allowSourceFallback;
        # eframe/glow: prefer the Wayland/EGL path inside Cage.
        WINIT_UNIX_BACKEND = "wayland";
        # Default to allowing software fallbacks; the kiosk wrapper will force
        # them when no usable DRM render node is detected at launch time.
        WLR_NO_HARDWARE_CURSORS = "1";
        WLR_RENDERER_ALLOW_SOFTWARE = "1";
        WLR_RENDERER = "soft";
        LIBGL_ALWAYS_SOFTWARE = "1";
        XDG_RUNTIME_DIR = "/run/user/1000";
      };
    };

    services.libinput.enable = true;
    # eframe/glow needs EGL/GL. The minimal installation-cd base does not set
    # this, so enable Mesa/DRM explicitly for the kiosk compositor. amdgpu and
    # virtio_gpu are loaded early so both the RX 580 and QEMU virtio-vga have a
    # DRM device by the time Cage starts.
    hardware.graphics.enable = true;
    # GPU modules/firmware are provided by hardware-universal.nix.
    security.polkit.enable = true;

    # The GUI runs as the unprivileged live `nixos` user and escalates only the
    # installer commands through sudo. This NOPASSWD policy is scoped to the
    # ephemeral live ISO and must never be copied to the target.
    security.sudo.wheelNeedsPassword = false;

    # Do NOT use lib.mkForce here. The installation-cd base contributes the
    # login/tty/shell packages; overriding the list is what removed bash and
    # caused the black screen/login loop. Merge our live tools with the base
    # set and make bashInteractive/coreutils explicit as a safety net.
    environment.systemPackages = with pkgs; [
      bashInteractive
      coreutils
      util-linux
      iproute2
      networkmanager
      parted
      gptfdisk
      dosfstools
      e2fsprogs
      btrfs-progs
      nixos-install-tools
      git
      curl
      foot
      mesa
      libglvnd
      hitechInstaller
      # Kept for the installer pipeline and live debugging.
      sudo
      nix
      efibootmgr
      efivar
      kmod
      hwdata
      pciutils
      usbutils
    ];

    # --- Deliberate size reductions --------------------------------------
    # The stock installation-cd base is built for every possible machine:
    # full Nixpkgs channel, all firmware, ZFS, software RAID, OpenSSH. The
    # NetInstaller needs none of that to reach the cache; the closures it
    # installs come from HiTech-Store. Wired Ethernet (and a curated set of
    # initrd storage/USB modules) is the supported live networking path.
    system.installer.channel.enable = false;
    system.extraDependencies = lib.mkForce [ ];
    # hardware-enableAllFirmware / vendor blobs are enabled by
    # hardware-universal.nix; do not force them off here.
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

    # Keep the installation-cd base default packages (installation-cd-base
    # sets this to [ rsync ]); do NOT force it empty. bash/coreutils are also
    # explicit systemPackages above.
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
    # No hardcoded live password: the kiosk session is passwordless by design
    # and the operator identity is created exclusively by the GUI form.
    users.users.nixos.initialHashedPassword = lib.mkForce "";
    users.users.nixos.extraGroups = [
      "wheel"
      "networkmanager"
      "video"
      "render"
      "input"
      "tty"
    ];

    # Always keep a usable shell reachable: getty@tty2/tty3 autologin as the
    # live user, and ttyS0 gets an explicit getty for serial/headless rescue.
    services.getty.autologinUser = "nixos";
    systemd.services."serial-getty@ttyS0" = {
      enable = true;
      wantedBy = [ "getty.target" ];
      environment.TERM = "linux";
      serviceConfig.ExecStart = [
        ""
        "${pkgs.util-linux}/bin/agetty --autologin nixos --login-program ${pkgs.shadow}/bin/login --noclear --keep-baud ttyS0 115200,38400,9600 linux"
      ];
    };

    # eframe/egui + Mesa are much smaller than the old Calamares/Qt/KDE stack,
    # but xz still compresses this closure noticeably better than zstd-19 at
    # the cost of a slower image build; the compressed ISO is what users get.
    isoImage.squashfsCompression = lib.mkDefault "xz -Xdict-size 100%";

    # --- Boot media identity ---------------------------------------------
    isoImage.volumeID = "HITECHOS_NET";
    isoImage.edition = "netinstaller";
    image.fileName = lib.mkForce
      "hitechos-netinstaller-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso";
  };
}
