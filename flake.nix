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

      # ISO live bootable sur matériel réel, générée à chaque push par la CI
      # (voir .github/workflows/ci.yml, job iso-build) :
      #   nix build .#nixosConfigurations.iso.config.system.build.isoImage
      nixosConfigurations.iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          # Module officiel NixOS — résolu via l'input nixpkgs du flake
          # (déterministe), plutôt que <nixpkgs/...> qui dépend de NIX_PATH.
          "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-minimal.nix"
          ./os/iso.nix
        ];
      };
    };
}
