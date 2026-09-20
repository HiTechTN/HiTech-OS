# HiTech-OS — remplace la combinaison officielle
# installation-cd-graphical-calamares-gnome.nix + calamares.nix par notre
# propre assemblage, pour pouvoir substituer notre Calamares patché
# (voir plus bas) au lieu du stock, sans dupliquer le paquet (ce qui
# provoquerait une collision de fichiers dans environment.systemPackages).
#
# Le contenu GNOME/polkit/locales ci-dessous est une transcription FIDÈLE
# des modules officiels NixOS (vérifiés sur nixpkgs, branche
# nixos-unstable, au moment de l'écriture) :
#   nixos/modules/installer/cd-dvd/installation-cd-graphical-calamares.nix
#   nixos/modules/installer/cd-dvd/installation-cd-graphical-calamares-gnome.nix
# Seule différence : les paquets Calamares stock sont remplacés par notre
# variante patchée (calamaresExtensionsHitechos ci-dessous), qui ajoute une
# étape d'installation supplémentaire déployant les services HiTech-OS.
# flake.nix importe cette fois installation-cd-graphical-base.nix
# directement (pas la chaîne complète officielle), pour éviter que le
# paquet Calamares stock soit tiré en plus du nôtre.
#
# TRANSPARENCE : voir le commentaire en tête de la section "Installeur"
# plus bas pour le détail (et les limites) de l'étape d'installation
# HiTech-OS elle-même — ce mécanisme n'a pas pu être testé (Calamares ne
# tourne jamais pendant `nix build`, seulement quand un humain boot l'ISO).

{ lib, pkgs, ... }:

let
  # --- Installeur : notre étape supplémentaire (services HiTech-OS) ---
  hitechosShellprocessConf = pkgs.writeText "shellprocess-hitechos.conf" ''
    ---
    # Commandes exécutées CHROOTÉES dans le système fraîchement installé
    # (dontChroot: false => ROOT == /, voir doc shellprocess de Calamares).
    dontChroot: false
    script:
      - "nix-shell -p git --run 'git clone --depth 1 https://github.com/HiTechTN/HiTech-OS.git /etc/nixos/hitech-os'"
      - "sed -i '/hardware-configuration.nix/a\\      /etc/nixos/hitech-os/os/common.nix' /etc/nixos/configuration.nix"
      - "/run/current-system/sw/bin/nixos-rebuild boot"
  '';

  calamaresExtensionsHitechos = pkgs.calamares-nixos-extensions.overrideAttrs (old: {
    postInstall = (old.postInstall or "") + ''
      cp ${hitechosShellprocessConf} $out/etc/calamares/modules/shellprocess-hitechos.conf

      sed -i \
        '/^  weight:   48$/a\- id:       hitechos\n  module:   shellprocess\n  config:   shellprocess-hitechos.conf' \
        $out/etc/calamares/settings.conf

      sed -i \
        '/^  - nixos$/a\  - shellprocess@hitechos' \
        $out/etc/calamares/settings.conf
    '';
  });

  calamaresNixosHitechos = pkgs.calamares-nixos.override {
    calamares-nixos-extensions = calamaresExtensionsHitechos;
  };

  # Lance Calamares automatiquement à l'ouverture de session GNOME —
  # transcrit de calamares.nix, avec notre paquet patché à la place du
  # stock (pkgs.calamares-nixos).
  calamaresAutostart = pkgs.makeAutostartItem {
    name = "calamares";
    package = calamaresNixosHitechos;
  };
in
{
  # --- Section équivalente à installation-cd-graphical-calamares.nix ---
  security.polkit.enable = true; # requis par pkexec (Calamares + nixos-install) — annule le mkDefault false de common.nix
  security.polkit.enablePkexecWrapper = true;
  programs.partition-manager.enable = true;
  environment.systemPackages = [
    calamaresNixosHitechos
    calamaresAutostart
    pkgs.glibcLocales
  ];
  i18n.supportedLocales = [ "all" ];

  # --- Section équivalente à installation-cd-graphical-calamares-gnome.nix ---
  isoImage.edition = lib.mkDefault "hitechos-gnome";

  services.desktopManager.gnome = {
    favoriteAppsOverride = ''
      [org.gnome.shell]
      favorite-apps=[ 'firefox.desktop', 'nixos-manual.desktop', 'org.gnome.Console.desktop', 'org.gnome.Nautilus.desktop', 'gparted.desktop', 'calamares.desktop', 'org.gnome.Software.desktop' ]
    '';
    extraGSettingsOverrides = ''
      [org.gnome.shell]
      welcome-dialog-last-shown-version='9999999999'
      [org.gnome.desktop.session]
      idle-delay=0
      [org.gnome.settings-daemon.plugins.power]
      sleep-inactive-ac-type='nothing'
      sleep-inactive-battery-type='nothing'
    '';
    extraGSettingsOverridePackages = [ pkgs.gnome-settings-daemon ];
    enable = true;
  };

  environment.variables = {
    QT_QPA_PLATFORM = "$([[ $XDG_SESSION_TYPE = \"wayland\" ]] && echo \"wayland\")";
  };

  services.displayManager.gdm = {
    enable = true;
    autoSuspend = false;
  };

  services.displayManager.autoLogin = {
    enable = true;
    user = "nixos";
  };
}
