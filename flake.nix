{
  description = "HiTech-OS — Sovereign, local-first, AI-first OS for IoT & Smart Infrastructure";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        # `nix develop` — environnement de dev pour le démon IA (Rust)
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rust-analyzer
            pkg-config
            mosquitto
          ];
        };

        # `nix build .#nixosConfigurations` sera défini une fois la
        # configuration NixOS de base (Phase 1) écrite dans ./os/
      }
    ) // {
      nixosConfigurations.patient-zero = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [ ./os/configuration.nix ];
      };
    };
}
