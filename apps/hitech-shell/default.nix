# Nix derivation — hitech-shell.
#
# The shell now links `calloop` (Smithay event-loop seam), so the old direct
# `rustc src/main.rs` build no longer works: it needs the dependency graph from
# Cargo.toml. `rustPlatform.buildRustPackage` fetches the pinned crates from
# Cargo.lock (checksum-verified), runs `cargo test` (layout/config unit tests)
# and installs the binary plus the Wayland session entry.
{
  lib,
  rustPlatform,
  makeWrapper,
  pkg-config,
  seatd,
  libinput,
  udev,
  libdrm,
  libgbm,
  wayland,
  libxkbcommon,
  mesa,
  libGL,
  xkeyboard-config,
}:

let
  sessionFile = builtins.toFile "hitech-shell.desktop" ''
    [Desktop Entry]
    Name=HiTech Shell
    Comment=HiTech-OS modular Wayland desktop shell
    Exec=hitech-shell
    Type=Application
    DesktopNames=HiTechShell
  '';
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "hitech-shell";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        base = baseNameOf path;
      in
      !(builtins.elem base [
        "target"
        "result"
      ]);
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ makeWrapper pkg-config ];

  buildInputs = [
    seatd
    seatd.dev
    libinput
    libinput.dev
    udev
    udev.dev
    libdrm
    libdrm.dev
    libgbm
    wayland
    wayland.dev
    libxkbcommon
    libxkbcommon.dev
    mesa
    libGL
    xkeyboard-config
  ];

  postInstall = ''
    # Wayland session entry consumed by services.displayManager.sessionPackages.
    install -Dm644 ${sessionFile} $out/share/wayland-sessions/hitech-shell.desktop

    # The compositor needs the XKB data at runtime, independent of the
    # display-manager environment.
    wrapProgram $out/bin/hitech-shell \
      --prefix XKB_CONFIG_ROOT : ${xkeyboard-config}/share/X11/xkb
  '';

  passthru.providedSessions = [ "hitech-shell" ];

  meta = {
    description = "HiTech-Shell — modular Wayland desktop shell for HiTech-OS";
    homepage = "https://github.com/HiTechTN/HiTech-OS";
    license = lib.licenses.agpl3Only;
    mainProgram = "hitech-shell";
    platforms = lib.platforms.linux;
  };
})
