# HiTech-OS — image ISO live/installeur graphique, pour test et
# installation sur matériel réel.
#
# La partie GNOME + Calamares (avec notre étape d'installation
# supplémentaire) vit dans ./calamares-hitechos.nix ; ce fichier-ci ne
# contient que les réglages propres à L'IMAGE ISO elle-même.
#
# ATTENTION (à bien comprendre avant de flasher) :
# - Contrairement à une install NixOS+Calamares générique, celle-ci
#   DÉPLOIE AUSSI les services HiTech-OS sur le disque installé (voir
#   calamares-hitechos.nix). Statut : un premier test réel sur matériel
#   physique (20/09/2026) a échoué AVANT d'atteindre cette étape (le job
#   Calamares standard "nixos" plantait sur une histoire de flakes
#   désactivés — corrigé dans common.nix) ; le mécanisme d'installation
#   HiTech-OS lui-même reste donc à valider au prochain test réel.
#   Teste d'abord dans une VM avant un disque réel si possible.
# - Cette image est volontairement graphique/lourde (GNOME, Firefox,
#   Calamares) : elle sert à tester/installer sur du matériel, ce n'est
#   PAS le profil headless <150 Mo RAM du nœud déployé (configuration.nix
#   + common.nix restent inchangés et headless pour le vrai déploiement
#   décrit dans le README).

{ config, lib, pkgs, ... }:

let
  hitechosStatusScript = pkgs.writeShellScriptBin "hitechos-status-launcher" ''
    systemctl status mosquitto telegraf influxdb2
    echo
    echo "Appuie sur Entrée pour fermer"
    read
  '';

  hitechosDocsScript = pkgs.writeShellScriptBin "hitechos-docs-launcher" ''
    less /etc/hitechos/ROADMAP.md
  '';

  hitechosStatusItem = pkgs.makeDesktopItem {
    name = "hitechos-status";
    desktopName = "HiTech-OS — État des services";
    comment = "Affiche l'état de Mosquitto, Telegraf et InfluxDB";
    icon = "utilities-system-monitor";
    exec = "gnome-terminal -- ${hitechosStatusScript}/bin/hitechos-status-launcher";
    terminal = false;
    categories = [ "System" ];
  };

  # ORION-X is the automatic welcome screen (and a permanent application
  # launcher) on the Live ISO. Calamares stays installed as a recovery path.
  hitechInstaller = pkgs.callPackage ../hitech-installer { };

  hitechInstallerDesktop = pkgs.makeDesktopItem {
    name = "hitech-installer";
    desktopName = "HiTech-OS Spaceship Installer";
    comment = "ORION-X Spaceship Cockpit — install HiTech-OS";
    icon = "system-software-install";
    exec = "${hitechInstaller}/bin/hitech-installer";
    terminal = false;
    categories = [ "System" "Settings" ];
  };

  hitechInstallerAutostart = pkgs.makeAutostartItem {
    name = "hitech-installer";
    package = hitechInstallerDesktop;
  };

  hitechosDocsItem = pkgs.makeDesktopItem {
    name = "hitechos-docs";
    desktopName = "HiTech-OS — Documentation (roadmap)";
    comment = "Roadmap et décisions d'architecture HiTech-OS, en lecture hors-ligne";
    icon = "help-contents";
    exec = "gnome-terminal -- ${hitechosDocsScript}/bin/hitechos-docs-launcher";
    terminal = false;
    categories = [ "Utility" ];
  };
in
{
  imports = [
    ./common.nix
    ./hardware-universal.nix
    ./calamares-hitechos.nix
  ];

  networking.hostName = "hitechos-live";

  # The live image tracks the release it was built from. This must be explicit:
  # common.nix only *defaults* stateVersion so an installed target's own
  # configuration.nix can win, and installation-cd-base already defaults it to
  # lib.trivial.release — two different mkDefault values would conflict.
  system.stateVersion = lib.mkForce config.system.nixos.release;

  # Stop UDisks2/GVFS from automounting target partitions under /run/media
  # while the installer is partitioning. The installer still unmounts
  # defensively, but this removes the race at the source.
  services.udisks2.enable = lib.mkForce false;
  services.gvfs.enable = lib.mkForce false;

  # Doc embarquée pour un accès hors-ligne (menu "HiTech-OS — Documentation")
  environment.etc."hitechos/ROADMAP.md".source = ../docs/ROADMAP.md;
  environment.systemPackages = [
    pkgs.gnome-software
    pkgs.git # live : le script Calamares l'utilise hors chroot (calamares-hitechos.nix)
    hitechosStatusItem
    hitechosDocsItem
    hitechInstallerDesktop
    hitechInstallerAutostart
  ];

  # --- Flatpak, préconfiguré avec Flathub (pas de remote-add manuel) ---
  services.flatpak.enable = true;
  systemd.services.hitechos-flathub-remote = {
    description = "Enregistrer le remote Flathub pour Flatpak";
    wantedBy = [ "multi-user.target" ];
    after = [ "flatpak-system-helper.service" ];
    path = [ pkgs.flatpak ];
    serviceConfig.Type = "oneshot";
    script = ''
      flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
    '';
  };

  # --- Accès console de secours (demande explicite : test sur matériel
  # réel sans dépendre du réseau/SSH pour se logger). Mot de passe FIXE
  # et volontairement faible : cette image est éphémère (live), ce
  # réglage ne doit JAMAIS être repris sur le nœud déployé
  # (configuration.nix n'importe pas ce fichier). ---
  users.users.nixos.initialHashedPassword = lib.mkForce null; # évite l'ambiguïté de précédence avec initialPassword ci-dessous
  users.users.nixos.initialPassword = "hitechos"; # compte live par défaut (autologin GNOME de toute façon)
  users.users.hitechos.initialPassword = "hitechos"; # notre compte, pour un accès TTY sans SSH

  image.fileName = lib.mkForce "hitechos-live-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso"; # isoImage.isoName a été renommé en amont
}
