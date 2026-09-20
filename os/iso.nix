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
#   calamares-hitechos.nix) — mais ce mécanisme n'a jamais été testé en
#   conditions réelles (Calamares ne tourne jamais pendant `nix build`).
#   Teste d'abord dans une VM avant un disque réel si possible.
# - Cette image est volontairement graphique/lourde (GNOME, Firefox,
#   Calamares) : elle sert à tester/installer sur du matériel, ce n'est
#   PAS le profil headless <150 Mo RAM du nœud déployé (configuration.nix
#   + common.nix restent inchangés et headless pour le vrai déploiement
#   décrit dans le README).

{ config, lib, pkgs, ... }:

let
  hitechosStatusItem = pkgs.makeDesktopItem {
    name = "hitechos-status";
    desktopName = "HiTech-OS — État des services";
    comment = "Affiche l'état de Mosquitto, Telegraf et InfluxDB";
    icon = "utilities-system-monitor";
    exec = "gnome-terminal -- bash -c 'systemctl status mosquitto telegraf influxdb2; echo; echo Appuie sur Entrée pour fermer; read'";
    terminal = false;
    categories = [ "System" ];
  };

  hitechosDocsItem = pkgs.makeDesktopItem {
    name = "hitechos-docs";
    desktopName = "HiTech-OS — Documentation (roadmap)";
    comment = "Roadmap et décisions d'architecture HiTech-OS, en lecture hors-ligne";
    icon = "help-contents";
    exec = "gnome-terminal -- bash -c 'less /etc/hitechos/ROADMAP.md'";
    terminal = false;
    categories = [ "Documentation" ];
  };
in
{
  imports = [
    ./common.nix
    ./calamares-hitechos.nix
  ];

  networking.hostName = "hitechos-live";

  # Doc embarquée pour un accès hors-ligne (menu "HiTech-OS — Documentation")
  environment.etc."hitechos/ROADMAP.md".source = ../docs/ROADMAP.md;
  environment.systemPackages = [
    pkgs.gnome-software
    hitechosStatusItem
    hitechosDocsItem
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
  users.users.nixos.initialPassword = "hitechos"; # compte live par défaut (autologin GNOME de toute façon)
  users.users.hitechos.initialPassword = "hitechos"; # notre compte, pour un accès TTY sans SSH

  isoImage.isoName = lib.mkForce "hitechos-live-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso";
}
