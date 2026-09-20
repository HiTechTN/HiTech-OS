# HiTech-OS — image ISO live/installeur graphique, pour test et
# installation sur matériel réel.
#
# ATTENTION (à bien comprendre avant de flasher) :
# - Ceci est un installeur NixOS graphique GENERIQUE (GNOME + Calamares,
#   modules officiels NixOS), pas un "installeur HiTech-OS en un clic".
#   Calamares installe un NixOS de base sur le disque ; pour obtenir
#   ensuite les services HiTech-OS (Mosquitto, Telegraf, durcissement),
#   il faut APRES l'installation :
#     git clone https://github.com/HiTechTN/HiTech-OS.git
#     cd HiTech-OS
#     sudo nixos-rebuild switch --flake .#patient-zero
# - Cette image est volontairement graphique/lourde (GNOME, Firefox,
#   Calamares) : elle sert à tester/installer sur du matériel, ce n'est
#   PAS le profil headless <150 Mo RAM du nœud déployé (configuration.nix
#   + common.nix restent inchangés et headless pour le vrai déploiement).
#
# Inclus par défaut par le module graphique officiel : Firefox, gparted,
# vim/nano — voir installation-cd-graphical-base.nix en amont.

{ config, lib, pkgs, ... }:

{
  imports = [
    ./common.nix
  ];

  networking.hostName = "hitechos-live";

  # --- Environnement graphique + installeur (GNOME + Calamares) ---
  # Le module officiel active lui-même services.xserver, GDM, autologin
  # sur l'utilisateur live "nixos" (déjà créé par le profil d'installation
  # NixOS standard) et le polkit nécessaire à Calamares/pkexec.
  # NB : le module lui-même (voir upstream) requiert d'être combiné avec
  # ./installation-cd-graphical-calamares-gnome.nix ; celui-ci est fourni
  # par flake.nix via l'input nixpkgs (cohérent avec l'approche déjà
  # utilisée pour installation-cd-minimal.nix — voir flake.nix).
  security.polkit.enable = true; # requis par Calamares (pkexec) — annule le mkDefault false de common.nix

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

  # GNOME Software = magasin d'applications graphique (Flatpak inclus),
  # pour une installation d'apps "facile" sans ligne de commande.
  environment.systemPackages = [ pkgs.gnome-software ];

  # --- Accès console de secours (demande explicite : test sur matériel
  # réel sans dépendre du réseau/SSH pour se logger). Mot de passe FIXE
  # et volontairement faible : cette image est éphémère (live), ce
  # réglage ne doit JAMAIS être repris sur le nœud déployé
  # (configuration.nix n'importe pas ce fichier). ---
  users.users.nixos.initialPassword = "hitechos"; # compte live par défaut (autologin GNOME de toute façon)
  users.users.hitechos.initialPassword = "hitechos"; # notre compte, pour un accès TTY sans SSH

  isoImage.isoName = lib.mkForce "hitechos-live-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso";
}
