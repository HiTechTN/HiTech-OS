# Installs the universal bootstrap entrypoint into the Nix store.
{ lib, stdenvNoCC }:
stdenvNoCC.mkDerivation {
  pname = "hitech-bootstrap";
  version = "1.0.0";
  src = ./.;

  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 hitech-bootstrap.sh "$out/bin/hitech-bootstrap"
    install -Dm644 hitech-bootstrap.ps1 "$out/share/hitech-bootstrap/hitech-bootstrap.ps1"
    install -Dm644 README.md "$out/share/doc/hitech-bootstrap/README.md"
    runHook postInstall
  '';

  meta = {
    description = "HiTech-OS universal bootstrap and morphing engine";
    homepage = "https://github.com/HiTechTN/HiTech-OS";
    license = lib.licenses.agpl3Only;
    maintainers = [ ];
    platforms = lib.platforms.linux;
  };
}
