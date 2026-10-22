# Nix derivation — hitech-store (portail universel FastAPI HiTechTN)
#
# Générée « à la app2nix » ( conventions de sortie : env Python complet,
# assets copiés dans $out/share, binaire uvicorn dans $out/bin ) —
# app2nix v3.0.0 (`app2nix convert`) cible les paquets BINAIRES (.deb,
# .rpm, .AppImage…) ; pour une application source Python comme celle-ci,
# on produit l'équivalent à la main, aligné sur le même layout de sortie.
#
# Usage isolé :  nix build .          (dans /data/hitech-store)
# Usage module : pkgs.callPackage /data/hitech-store/default.nix {}
{ pkgs ? import <nixpkgs> { } }:

let
  pythonEnv = pkgs.python3.withPackages (
    ps:
    with ps;
    [
      fastapi
      uvicorn
      python-dotenv
      python-multipart
      jinja2
      pypdf
      python-docx
      openpyxl
      python-pptx
    ]
    # « python-jose » contient un tiret : accès explicite (sinon
    # `with ps; python-jose` serait lu comme une soustraction Nix).
    ++ [ ps.python-jose ]
  );
in
pkgs.stdenv.mkDerivation {
  pname = "hitech-store";
  version = "1.0.0";

  # Copie minimale et pure : app.py + static/ + templates/.
  # Exclut venv/, .env (secret !), storage/, logs/, caches, état runtime.
  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        base = baseNameOf path;
      in
      if builtins.elem base [
        "venv"
        ".venv"
        "__pycache__"
        "logs"
        "storage"
        ".office_cache"
        "edge"
        ".git"
        "node_modules"
      ] then
        false
      else if type == "directory" then
        true
      else
        builtins.elem base [
          "app.py"
          "requirements.txt"
        ]
        || pkgs.lib.hasSuffix ".py" base
        || pkgs.lib.hasSuffix ".html" base
        || pkgs.lib.hasSuffix ".css" base
        || pkgs.lib.hasSuffix ".js" base;
  };

  nativeBuildInputs = [ pkgs.makeWrapper ];

  postPatch = ''
    # Le store Nix est en lecture seule : l'état runtime (OTP, liens de
    # partage, logs, cache Office) doit pouvoir être redirigé vers
    # /data/hitech-store. Les 7 chemins deviennent surchargeables par
    # variable d'environnement (défaut = comportement d'origine, sans
    # régression pour pip/venv).
    substituteInPlace app.py \
      --replace-fail 'LOG_DIR = BASE_DIR / "logs"' \
        'LOG_DIR = Path(os.getenv("LOG_DIR", str(BASE_DIR / "logs")))' \
      --replace-fail 'OTP_STATE_FILE = BASE_DIR / ".otp_state.json"' \
        'OTP_STATE_FILE = Path(os.getenv("OTP_STATE_FILE", str(BASE_DIR / ".otp_state.json")))' \
      --replace-fail 'OTP_LOCK_FILE = BASE_DIR / ".otp_state.lock"' \
        'OTP_LOCK_FILE = Path(os.getenv("OTP_LOCK_FILE", str(BASE_DIR / ".otp_state.lock")))' \
      --replace-fail 'SHARES_FILE = BASE_DIR / ".shares.json"' \
        'SHARES_FILE = Path(os.getenv("SHARES_FILE", str(BASE_DIR / ".shares.json")))' \
      --replace-fail 'SHARES_LOCK_FILE = BASE_DIR / ".shares.lock"' \
        'SHARES_LOCK_FILE = Path(os.getenv("SHARES_LOCK_FILE", str(BASE_DIR / ".shares.lock")))' \
      --replace-fail 'OFFICE_CACHE_DIR = BASE_DIR / ".office_cache"' \
        'OFFICE_CACHE_DIR = Path(os.getenv("OFFICE_CACHE_DIR", str(BASE_DIR / ".office_cache")))' \
      --replace-fail 'OFFICE_LOCK_FILE = BASE_DIR / ".office.lock"' \
        'OFFICE_LOCK_FILE = Path(os.getenv("OFFICE_LOCK_FILE", str(BASE_DIR / ".office.lock")))'
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/hitech-store $out/bin
    cp app.py $out/share/hitech-store/
    cp -r static templates $out/share/hitech-store/

    # Wrapper = uvicorn du env Python Nix. --app-dir place app.py sur
    # sys.path ; --chdir fixe le cwd sur les assets (StaticFiles/Jinja2
    # résolvent déjà via __file__, double sécurité).
    makeWrapper ${pythonEnv}/bin/uvicorn $out/bin/hitech-store \
      --prefix PATH : ${pythonEnv}/bin \
      --chdir $out/share/hitech-store \
      --add-flags "--app-dir $out/share/hitech-store"
    runHook postInstall
  '';

  meta = {
    description = "Portail universel HiTechTN — stockage, partage, preview et extraction de documents (FastAPI)";
    platforms = pkgs.lib.platforms.linux;
  };
}
