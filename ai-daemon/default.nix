# Nix derivation — hitechos-ai-daemon.
#
# Default build is the dependency-light stub backend (no llama.cpp toolchain),
# which is what CI and the cache builder validate. Enable the real CPU backend
# with a local `cargo build --features cpu` on a host that has cmake + a C++
# compiler; the CUDA backend stays a feature flag (see docs/ROADMAP.md Phase 4).
{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "hitechos-ai-daemon";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      let
        base = baseNameOf (toString path);
      in
      base != "target" && base != "result" && base != ".git";
  };

  cargoLock.lockFile = ./Cargo.lock;

  # Stub-backend tests + clippy-clean code are enforced by the CI rust job;
  # running them here too keeps `nix flake check` honest.
  doCheck = true;

  meta = with lib; {
    description = "HiTech-OS AI daemon — pluggable GGUF inference backend (stub by default)";
    homepage = "https://github.com/HiTechTN/HiTech-OS";
    license = licenses.agpl3Plus;
    mainProgram = "hitechos-ai-daemon";
    platforms = platforms.linux;
  };
}
