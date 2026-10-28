{ stdenvNoCC, esbuild }:

stdenvNoCC.mkDerivation {
  pname = "hitech-installer-frontend";
  version = "2.0.0";
  src = ./frontend;

  nativeBuildInputs = [ esbuild ];

  buildPhase = ''
    runHook preBuild
    esbuild main.ts --bundle --minify --target=es2020 --outfile=main.js
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    install -m 0644 index.html styles.css main.js $out/
    runHook postInstall
  '';
}
