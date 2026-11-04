{
  description = "HiTech-OS — Sovereign, reproducible edge operating system for IoT & Smart Infrastructure";

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
        # `nix develop` — Rust, Wayland and telemetry development shell.
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rust-analyzer
            pkg-config
            mosquitto
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
        };

        packages = {
          default = pkgs.callPackage ./apps/hitech-shell { };
          hitech-shell = pkgs.callPackage ./apps/hitech-shell { };
          hitech-installer = pkgs.callPackage ./hitech-installer { };
          ai-daemon = pkgs.callPackage ./ai-daemon { };
          genio = pkgs.callPackage ./apps/genio { };
          hitech-store = pkgs.callPackage ./apps/hitech-store { };
          # Portable dependencies installed by bootstrap/hitech-bootstrap.sh
          # on Debian/Arch/Fedora/WSL2 so the runtime is not tied to the host
          # distribution's package versions.
          foot = pkgs.foot;
          mosquitto = pkgs.mosquitto;
          influxdb2 = pkgs.influxdb2;
          telegraf = pkgs.telegraf;
          hitech-bootstrap = pkgs.callPackage ./bootstrap { };
        };
      }
    ) // (let
      # Universal bootstrap targets. On a NixOS host, hitech-bootstrap copies
      # the host's hardware/configuration files into this flake tree as
      # host/configuration.nix before running:
      #   nixos-rebuild boot --flake /etc/nixos/hitech-os#orion
      # The if-expression is evaluated relative to the flake source, so it is
      # pure-eval safe both in CI and on the target.
      hostConfiguration =
        if builtins.pathExists ./host/configuration.nix
        then ./host/configuration.nix
        else if builtins.pathExists ./host-configuration.nix
        then ./host-configuration.nix
        else ./os/configuration.nix;
      mkProfile = profile: nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          self = self;
          hitechosRevision = self.rev or "main";
        };
        modules = [
          hostConfiguration
          (./os/profiles + "/${profile}.nix")
        ];
      };
    in {
      # Phase 8 NetInstaller: minimal Cage/ORION-X ISO with the complete
      # source tree at /etc/hitech-os-source and the target closure from the
      # HiTech-Store/Harmonia binary cache. `self.rev` is baked in as the
      # default repoRef so release builds are reproducible.
      nixosConfigurations.netinstaller = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          self = self;
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
        specialArgs = { self = self; };
        modules = [ ./os/configuration.nix ];
      };

      # Universal morphing profiles consumed by bootstrap/hitech-bootstrap.sh.
      nixosConfigurations.orion = mkProfile "orion";
      nixosConfigurations.edge = mkProfile "edge";
      nixosConfigurations.server = mkProfile "server";
      nixosConfigurations.minimal = mkProfile "minimal";

      # ISO live/installeur graphique (GNOME + notre Calamares patché +
      # Firefox + Flatpak), générée à chaque push par la CI (voir
      # .github/workflows/ci.yml, job iso-build) :
      #   nix build .#nixosConfigurations.iso.config.system.build.isoImage
      nixosConfigurations.iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { self = self; };
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
    });
}
