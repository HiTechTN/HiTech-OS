# HiTech-OS — installeur "one-click" : greffe une étape supplémentaire
# sur l'installeur graphique Calamares officiel de NixOS, pour que
# l'installation sur disque inclue AUSSI nos services (common.nix), pas
# juste un NixOS générique.
#
# HISTORIQUE / POURQUOI CETTE VERSION (v2) :
# La première version patchait settings.conf avec `sed` (a\ + \n) dans le
# postInstall du dérivé. Testé en réel le 20/09/2026 sur du matériel
# physique (voir la conversation) : le sed n'a PRIS AUCUN EFFET dans le
# bac à sable de build Nix (aucune erreur, mais notre job "shellprocess@
# hitechos" n'apparaissait tout simplement pas dans la séquence exécutée
# par Calamares — silencieusement ignoré). Cause probable : GNU sed
# interprète \n dans le texte d'une commande `a\` différemment selon le
# contexte d'exécution, fragile à travers les sandboxes de build.
#
# Cette version (v2) élimine complètement le sed : le settings.conf
# complet est écrit tel quel en Nix (chaîne littérale ci-dessous),
# construit à partir du contenu RÉEL de calamares-nixos-extensions au
# commit nixpkgs épinglé dans flake.lock (20b1ddd1aa5ace70c9468305030aa
# 4f9ef79671b — la révision exacte qui a servi à builder l'ISO testée le
# 20/09/2026), avec seulement les deux ajouts nécessaires (une instance
# + une entrée de séquence). Zéro ambiguïté d'échappement shell : soit
# ça compile avec le bon contenu, soit `nix build` échoue directement.
#
# TRANSPARENCE : voir aussi le premier test réel — l'erreur rencontrée
# ("experimental Nix feature flakes is disabled") venait du job STOCK
# "nixos" (nixos-install), pas de ce fichier : corrigée séparément dans
# common.nix (nix.settings.experimental-features). Cette étape
# shellprocess elle-même n'a PAS ENCORE été testée en conditions réelles
# (le premier test a échoué avant d'atteindre ce point de la séquence,
# et de toute façon la version sed ne s'exécutait pas). Prochain test
# réel à surveiller.
#
# Mécanisme (une fois que ça s'exécute) :
# 1. Le job "nixos" (natif Calamares) partitionne, installe un NixOS de
#    base et écrit /etc/nixos/{configuration,hardware-configuration}.nix
#    — on ne touche PAS à ça, ce sont les réglages propres à CE disque
#    précis (bootloader EFI/BIOS, UUIDs de partitions...).
# 2. Notre étape "shellprocess" s'exécute juste après, chrootée dans le
#    système fraîchement installé (dontChroot: false) :
#    a. clone HiTech-OS dans /etc/nixos/hitech-os
#    b. insère l'import de os/common.nix (services, durcissement — AUCUN
#       réglage disque/bootloader dedans, donc pas de conflit avec ce que
#       "nixos" vient d'écrire) juste après la ligne
#       "./hardware-configuration.nix" du configuration.nix généré
#    c. `nixos-rebuild boot` pour que ce soit actif dès le premier
#       démarrage réel (pas besoin de "switch" : le système n'est pas
#       encore démarré, on ne fait que fixer la génération de boot par
#       défaut)

{ pkgs, ... }:

let
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

  hitechosSettingsConf = pkgs.writeText "settings.conf" ''
    # Configuration file for Calamares
    #
    # This is the top-level configuration file for Calamares.
    # It specifies what modules will be used, as well as some
    # overall characteristics -- is this a setup program, or
    # an installer. More specific configuration is devolved
    # to the branding file (for the UI) and the individual
    # module configuration files (for functionality).
    ---
    # Modules can be job modules (with different interfaces) and QtWidgets view
    # modules. They could all be placed in a number of different paths.
    # "modules-search" is a list of strings, each of these can either be a full
    # path to a directory or the keyword "local".
    #
    # "local" means:
    #   - modules in $LIBDIR/calamares/modules, with
    #   - settings in SHARE/calamares/modules or /etc/calamares/modules.
    # In debug-mode (e.g. calamares -d) "local" also adds some paths
    # that make sense from inside the build-directory, so that you
    # can build-and-run with the latest modules immediately.
    #
    # Strings other than "local" are taken as paths and interpreted
    # relative to wherever Calamares is started. It is therefore **strongly**
    # recommended to use only absolute paths here. This is mostly useful
    # if your distro has forks of standard Calamares modules, but also
    # uses some form of upstream packaging which might overwrite those
    # forked modules -- then you can keep modules somewhere outside of
    # the "regular" module tree.
    #
    #
    # YAML: list of strings.
    modules-search: [ local, @out@/lib/calamares/modules ]

    # Instances section. This section is optional, and it defines custom instances
    # for modules of any kind. An instance entry has these keys:
    # - *module* name, which matches the module name from the module descriptor
    #   (usually the name of the directory under `src/modules/`, but third-
    #   party modules may diverge.
    # - *id* (optional) an identifier to distinguish this instance from
    #   all the others. If none is given, the name of the module is used.
    #   Together, the module and id form an instance key (see below).
    # - *config* (optional) a filename for the configuration. If none is
    #   given, *module*`.conf` is used (e.g. `welcome.conf` for the welcome
    #   module)
    # - *weight* (optional) In the *exec* phase of the sequence, progress
    #   is reported as jobs are completed. The jobs from a single module
    #   together contribute the full weight of that module. The overall
    #   progress (0 .. 100%) is divided up according to the weight of each
    #   module. Give modules that take a lot of time to complete, a larger
    #   weight to keep the overall progress moving along steadily. This
    #   weight overrides a weight given in the module descriptor. If no weight
    #   is given, uses the value from the module descriptor, or 1 if there
    #   isn't one there either.
    #
    # The primary goal of this mechanism is to allow loading multiple instances
    # of the same module, with different configuration. If you don't need this,
    # the instances section can safely be left empty.
    #
    # Module name plus instance name makes an instance key, e.g.
    # "webview@owncloud", where "webview" is the module name (for the webview
    # viewmodule) and "owncloud" is the instance name. In the *sequence*
    # section below, use instance-keys to name instances (instead of just
    # a module name, for modules which have only a single instance).
    #
    # Every module implicitly has an instance with the instance name equal
    # to its module name, e.g. "welcome@welcome". In the *sequence* section,
    # mentioning a module without a full instance key (e.g. "welcome")
    # means that implicit module.
    #
    # An instance may specify its configuration file (e.g. `webview-home.conf`).
    # The implicit instances all have configuration files named `<module>.conf`.
    # This (implict) way matches the source examples, where the welcome
    # module contains an example `welcome.conf`. Specify a *config* for
    # any module (also implicit instances) to change which file is used.
    #
    # For more information on running module instances, run Calamares in debug
    # mode and check the Modules page in the Debug information interface.
    #
    # A module that is often used with instances is shellprocess, which will
    # run shell commands specified in the configuration file. By configuring
    # more than one instance of the module, multiple shell sessions can be run
    # during install.
    #
    # YAML: list of maps of string:string key-value pairs.
    instances:
    - id:       unfree
      module:   notesqml
      config:   unfree.conf
    - module:   nixos
      weight:   48
    - id:       hitechos
      module:   shellprocess
      config:   shellprocess-hitechos.conf

    # Sequence section. This section describes the sequence of modules, both
    # viewmodules and jobmodules, as they should appear and/or run.
    #
    # A jobmodule instance key (or name) can only appear in an exec phase, whereas
    # a viewmodule instance key (or name) can appear in both exec and show phases.
    # There is no limit to the number of show or exec phases. However, the same
    # module instance key should not appear more than once per phase, and
    # deployers should take notice that the global storage structure is persistent
    # throughout the application lifetime, possibly influencing behavior across
    # phases. A show phase defines a sequence of viewmodules (and therefore
    # pages). These viewmodules can offer up jobs for the execution queue.
    #
    # An exec phase displays a progress page (with brandable slideshow). This
    # progress page iterates over the modules listed in the *immediately
    # preceding* show phase, and enqueues their jobs, as well as any other jobs
    # from jobmodules, in the order defined in the current exec phase.
    #
    # It then executes the job queue and clears it. If a viewmodule offers up a
    # job for execution, but the module name (or instance key) isn't listed in the
    # immediately following exec phase, this job will not be executed.
    #
    # YAML: list of lists of strings.
    sequence:
    - show:
      - welcome
      - locale
      - keyboard
      - users
      - packagechooser
      - notesqml@unfree
      - partition
      - summary
    - exec:
      - partition
      - mount
      - nixos
      - shellprocess@hitechos
      - users
      - umount
    - show:
      - finished

    # A branding component is a directory, either in SHARE/calamares/branding or
    # in /etc/calamares/branding (the latter takes precedence). The directory must
    # contain a YAML file branding.desc which may reference additional resources
    # (such as images) as paths relative to the current directory.
    #
    # A branding component can also ship a QML slideshow for execution pages,
    # along with translation files.
    #
    # Only the name of the branding component (directory) should be specified
    # here, Calamares then takes care of finding it and loading the contents.
    #
    # YAML: string.
    branding: nixos

    # If this is set to true, Calamares will show an "Are you sure?" prompt right
    # before each execution phase, i.e. at points of no return. If this is set to
    # false, no prompt is shown. Default is false, but Calamares will complain if
    # this is not explicitly set.
    #
    # YAML: boolean.
    prompt-install: false

    # If this is set to true, Calamares will execute all target environment
    # commands in the current environment, without chroot. This setting should
    # only be used when setting up Calamares as a post-install configuration tool,
    # as opposed to a full operating system installer.
    #
    # Some official Calamares modules are not expected to function with this
    # setting. (e.g. partitioning seems like a bad idea, since that is expected to
    # have been done already)
    #
    # Default is false (for a normal installer), but Calamares will complain if
    # this is not explicitly set.
    #
    # YAML: boolean.
    dont-chroot: false

    # If this is set to true, Calamares refers to itself as a "setup program"
    # rather than an "installer". Defaults to the value of dont-chroot, but
    # Calamares will complain if this is not explicitly set.
    oem-setup: false

    # If this is set to true, the "Cancel" button will be disabled entirely.
    # The button is also hidden from view.
    #
    # This can be useful if when e.g. Calamares is used as a post-install
    # configuration tool and you require the user to go through all the
    # configuration steps.
    #
    # Default is false, but Calamares will complain if this is not explicitly set.
    #
    # YAML: boolean.
    disable-cancel: false

    # If this is set to true, the "Cancel" button will be disabled once
    # you start the 'Installation', meaning there won't be a way to cancel
    # the Installation until it has finished or installation has failed.
    #
    # Default is false, but Calamares will complain if this is not explicitly set.
    #
    # YAML: boolean.
    disable-cancel-during-exec: false

    # If this is set to true, the "Next" and "Back" button will be hidden once
    # you start the 'Installation'.
    #
    # Default is false, but Calamares will complain if this is not explicitly set.
    #
    # YAML: boolean.
    hide-back-and-next-during-exec: false

    # If this is set to true, then once the end of the sequence has
    # been reached, the quit (done) button is clicked automatically
    # and Calamares will close. Default is false: the user will see
    # that the end of installation has been reached, and that things are ok.
    #
    #
    quit-at-end: false
  '';

  # Surcharge du paquet de config Calamares : notre settings.conf complet
  # remplace le sien (pas de sed, pas de patch partiel — voir commentaire
  # d'en-tête), et notre fichier de commandes shellprocess est copié à
  # côté des autres fichiers de modules.
  calamaresExtensionsHitechos = pkgs.calamares-nixos-extensions.overrideAttrs (old: {
    postInstall = (old.postInstall or "") + ''
      cp ${hitechosShellprocessConf} $out/etc/calamares/modules/shellprocess-hitechos.conf
      cp ${hitechosSettingsConf} $out/etc/calamares/settings.conf
      # CRITIQUE : notre cp ci-dessus écrase settings.conf APRÈS que le
      # postInstall d'origine ait déjà fait
      # `substituteInPlace ... --replace-fail @out@ $out`. Notre copie
      # réintroduit donc le placeholder brut "@out@" dans
      # modules-search, ce qui empêche Calamares de trouver SES PROPRES
      # modules (nixos, shellprocess, etc.) — observé en réel le
      # 21/09/2026 : "FATAL: no sequence set" au lancement de Calamares.
      # On refait donc la même substitution sur notre copie.
      substituteInPlace $out/etc/calamares/settings.conf --replace-fail '@out@' "$out"
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
  isoImage.volumeID = pkgs.lib.mkForce "HITECHOS-LIVE";

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
