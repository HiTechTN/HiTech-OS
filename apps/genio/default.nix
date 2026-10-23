# Nix derivation — Genio (daemon FastAPI + dashboard React/Vite + CLI Rich)
#
# Layout de sortie (contrat lu par genio_server/daemon.py) :
#   $out/share/genio/genio_server/   → package Python importable
#   $out/share/genio/genio_web/dist/ → dashboard compilé (WEB_DIST)
#   $out/bin/genio-daemon            → wrapper `python -m genio_server.daemon`
#   $out/bin/genio                   → CLI interactive (Rich)
#
# Le daemon cherche son UI via :
#   Path(__file__).resolve().parent.parent / "genio_web" / "dist"
# soit $out/share/genio/genio_web/dist — ne pas déplacer ces répertoires.
{ pkgs ? import <nixpkgs> { } }:

let
  lib = pkgs.lib;

  pythonEnv = pkgs.python3.withPackages (
    ps: with ps; [
      fastapi
      uvicorn
      psutil
      rich
    ]
  );

  genioWeb = pkgs.buildNpmPackage {
    pname = "genio-web";
    version = "0.6.0";

    src = lib.cleanSourceWith {
      src = ./genio_web;
      filter =
        path: type:
        let
          base = baseNameOf path;
        in
        !(builtins.elem base [
          "node_modules"
          "dist"
          ".vite"
          ".git"
          "__pycache__"
        ]);
    };

    # sha256 obtenu avec `prefetch-npm-deps genio_web/package-lock.json`.
    npmDepsHash = "sha256-0nFf8Gw8reTnfUpVo+QdSloLS7/GNT0P2Hf9tEqjdeA=";

    # `npm run build` exécute `tsc --noEmit && vite build`.
    installPhase = ''
      runHook preInstall
      mkdir -p $out/share/genio-web
      cp -r dist/. $out/share/genio-web/
      runHook postInstall
    '';

    meta = {
      description = "Genio dashboard (React/Vite)";
      platforms = lib.platforms.linux;
    };
  };
in
pkgs.stdenv.mkDerivation {
  pname = "genio";
  version = "0.6.0-phase6";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        base = baseNameOf path;
      in
      !(builtins.elem base [
        ".git"
        "node_modules"
        "dist"
        ".vite"
        "__pycache__"
        ".pytest_cache"
        "genio_web"
        "tests"
      ])
      && (type == "directory"
        || lib.hasSuffix ".py" base
        || base == "requirements.txt"
        || base == "README.md");
  };

  nativeBuildInputs = [ pkgs.makeWrapper ];

  installPhase = ''
    runHook preInstall

    mkdir -p $out/share/genio $out/bin
    cp -r genio_server genio_cli $out/share/genio/
    find $out/share/genio -type d -name __pycache__ -prune -exec rm -rf {} +
    find $out/share/genio -type f -name '*.pyc' -delete

    mkdir -p $out/share/genio/genio_web
    cp -r ${genioWeb}/share/genio-web $out/share/genio/genio_web/dist

    makeWrapper ${pythonEnv}/bin/python $out/bin/genio-daemon \
      --prefix PYTHONPATH : "$out/share/genio" \
      --add-flags "-m genio_server.daemon"

    makeWrapper ${pythonEnv}/bin/python $out/bin/genio \
      --prefix PYTHONPATH : "$out/share/genio" \
      --add-flags "$out/share/genio/genio_cli/genio.py"

    runHook postInstall
  '';

  meta = {
    description = "Genio — daemon local, dashboard et CLI (HiTechTN)";
    platforms = lib.platforms.linux;
    mainProgram = "genio-daemon";
  };
}
