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

        packages = {
          default = pkgs.callPackage ./apps/hitech-shell { };
          hitech-shell = pkgs.callPackage ./apps/hitech-shell { };
        };
      }
    ) // {
      # Phase 8 NetInstaller: minimal Calamares ISO that pulls the HiTech-OS
      # configuration from GitHub and the target closure from the
      # HiTech-Store/Harmonia binary cache. `self.rev` is baked in as the
      # default repoRef so release builds are reproducible.
      nixosConfigurations.netinstaller = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          hitechosRevision = self.rev or "main";
        };
        modules = [
          # Base installer (channel, iso-image, installer-device) without the
          # `profiles/minimal.nix` repair toolkit. os/netinstaller.nix trims
          # the base further and adds only Calamares + networking + git.
          "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-base.nix"
          ./os/netinstaller.nix
        ];
      };

      nixosConfigurations.patient-zero = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [ ./os/configuration.nix ];
      };

      # ISO live/installeur graphique (GNOME + notre Calamares patché +
      # Firefox + Flatpak), générée à chaque push par la CI (voir
      # .github/workflows/ci.yml, job iso-build) :
      #   nix build .#nixosConfigurations.iso.config.system.build.isoImage
      nixosConfigurations.iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          # Module officiel NixOS — résolu via l'input nixpkgs du flake
          # (déterministe), plutôt que <nixpkgs/...> qui dépend de NIX_PATH.
          # Seul le module de BASE est importé ici (pas la chaîne
          # calamares/gnome stock) : le GNOME + Calamares (patché avec notre
          # étape d'installation HiTech-OS) vit dans os/calamares-hitechos.nix,
          # pour éviter d'installer le paquet Calamares stock EN PLUS du
          # nôtre (collision de fichiers sinon).
          "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-graphical-base.nix"
          ./os/iso.nix
        ];
      };
    };
}
