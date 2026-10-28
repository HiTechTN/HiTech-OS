{
  lib,
  pkgs,
  rustPlatform,
  pkg-config,
  makeWrapper,
  bashInteractive,
  coreutils,
  util-linux,
  networkmanager,
  iproute2,
  parted,
  dosfstools,
  e2fsprogs,
  nixos-install-tools,
  git,
  foot,
  nix,
  chromium,
  mkpasswd,
  openssl,
  mesa,
  libglvnd,
  libGL,
  libxkbcommon,
  wayland,
  libx11,
  libxcursor,
  libxi,
  libxrandr,
}:

let
  frontend = pkgs.callPackage ./frontend.nix { };
in
rustPlatform.buildRustPackage {
  pname = "hitech-installer";
  version = "2.0.0";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      let
        base = baseNameOf (toString path);
      in
      base != "target" && base != ".git";
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];

  buildInputs = [
    libGL
    libglvnd
    mesa
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];

  # eframe/glow dlopens the graphics stack at runtime. Keep the library path
  # wrapped so the binary also works outside the NixOS module's session.
  # PATH is prepended with the tools the installer shells out to so it does
  # not depend on the systemd service's minimal PATH. /run/wrappers/bin makes
  # the setuid sudo wrapper discoverable without pinning a non-setuid package.
  postInstall = ''
    mkdir -p "$out/share/hitech-installer/www"
    cp -r ${frontend}/. "$out/share/hitech-installer/www/"

    wrapProgram "$out/bin/hitech-installer" \
      --prefix PATH : "${
        lib.makeBinPath [
          bashInteractive
          coreutils
          util-linux
          networkmanager
          iproute2
          parted
          dosfstools
          e2fsprogs
          nixos-install-tools
          git
          foot
          nix
          chromium
          mkpasswd
          openssl
        ]
      }:/run/wrappers/bin" \
      --set HITECH_WEB_ROOT "$out/share/hitech-installer/www" \
      --prefix LD_LIBRARY_PATH : "${
        lib.makeLibraryPath [
          libGL
          libglvnd
          mesa
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ]
      }" \
      --run '
        soft=0
        if [ ! -e /dev/dri/renderD0 ] && [ ! -e /dev/dri/card0 ]; then
          soft=1
        fi
        if [ -e /sys/module/virtio_gpu ] || [ -e /sys/module/vboxguest ]; then
          soft=1
        fi
        if [ "$soft" = "1" ]; then
          export LIBGL_ALWAYS_SOFTWARE=1
          export WLR_RENDERER_ALLOW_SOFTWARE=1
          export WLR_RENDERER=soft
          export EGL_PLATFORM=x11
        fi
      '
  '';

  # The installer is an interactive GUI; cargo test has no headless suite.
  doCheck = false;

  meta = with lib; {
    description = "HiTech-OS ORION-X Spaceship Cockpit installer (Rust core + TypeScript HUD)";
    mainProgram = "hitech-installer";
    platforms = platforms.linux;
  };
}
