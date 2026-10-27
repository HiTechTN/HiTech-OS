{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
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
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];

  # eframe/glow dlopens the graphics stack at runtime. Keep the library path
  # wrapped so the binary also works outside the NixOS module's session.
  postInstall = ''
    wrapProgram "$out/bin/hitech-installer" \
      --prefix LD_LIBRARY_PATH : "${
        lib.makeLibraryPath [
          libGL
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ]
      }"
  '';

  # The installer is an interactive GUI; cargo test has no headless suite.
  doCheck = false;

  meta = with lib; {
    description = "HiTech-OS Cybernetic OS Studio graphical installer (eframe/egui)";
    mainProgram = "hitech-installer";
    platforms = platforms.linux;
  };
}
