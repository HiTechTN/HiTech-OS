{
  description = "Environnement Vibe-OS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs { inherit system overlays; };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          (rust-bin.nightly.latest.default.override {
            extensions = [ "rust-src" "llvm-tools-preview" ];
          })
          pkgs.stdenv.cc
          pkgs.qemu
          pkgs.pkg-config
        ];

        shellHook = ''
          echo "🦀 Environnement Vibe-OS chargé."
          echo "Astuce : Si cargo run échoue, tape 'cargo install bootimage'"
        '';
      };
    };
}