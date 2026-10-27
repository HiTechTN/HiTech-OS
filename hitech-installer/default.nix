{
  lib,
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
        ]
      }:/run/wrappers/bin" \
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
        if [ ! -e /dev/dri/card0 ] && [ ! -e /dev/dri/renderD128 ]; then
          export LIBGL_ALWAYS_SOFTWARE=1
        fi
      '
  '';

  # The installer is an interactive GUI; cargo test has no headless suite.
  doCheck = false;

  meta = with lib; {
    description = "HiTech-OS Cybernetic OS Studio graphical installer (eframe/egui)";
    mainProgram = "hitech-installer";
    platforms = platforms.linux;
  };
}
