# Nix derivation — hitech-shell (Phase 8 scaffold).
#
# The scaffold is deliberately dependency-free: `rustc` is invoked directly so
# the derivation builds on the cache builder without a vendored Cargo registry.
# When Smithay/Slint/Iced land, switch this file to
# `rustPlatform.buildRustPackage { cargoLock.lockFile = ./Cargo.lock; ... }`
# and let the binary cache publish the heavier closure.
{
  lib,
  stdenv,
  rustc,
  makeWrapper,
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
stdenv.mkDerivation (finalAttrs: {
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

  nativeBuildInputs = [ makeWrapper ];

  buildPhase = ''
    runHook preBuild

    ${rustc}/bin/rustc \
      --edition=2021 \
      --crate-name hitech_shell \
      -C opt-level=2 \
      -C debuginfo=1 \
      src/main.rs \
      -o hitech-shell

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    install -Dm755 hitech-shell $out/bin/hitech-shell

    # Wayland session entry consumed by services.displayManager.sessionPackages
    # (GDM/SDDM/greetd/regreet) once the compositor is real.
    install -Dm644 ${sessionFile} $out/share/wayland-sessions/hitech-shell.desktop

    runHook postInstall
  '';

  passthru.providedSessions = [ "hitech-shell" ];

  meta = {
    description = "HiTech-Shell — modular Wayland desktop shell for HiTech-OS (scaffold)";
    homepage = "https://github.com/HiTechTN/HiTech-OS";
    license = lib.licenses.agpl3Only;
    mainProgram = "hitech-shell";
    platforms = lib.platforms.linux;
  };
})
