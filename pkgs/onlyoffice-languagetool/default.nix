{
  lib,
  stdenvNoCC,
  fetchzip,
}:
stdenvNoCC.mkDerivation {
  pname = "onlyoffice-languagetool";
  version = "1.0.6";

  # The marketplace publishes a zip archive with the .plugin extension.
  src = fetchzip {
    url = "https://github.com/ONLYOFFICE/onlyoffice.github.io/releases/download/v0.0.112/languagetool.plugin";
    hash = "sha256-eNcXLZ+YsomtvS6Sz8XjZq8TgPVSPNhFWovD8d9XiOI=";
    extension = "zip";
    stripRoot = false;
  };

  installPhase = ''
    runHook preInstall
    install -d "$out/share/onlyoffice-languagetool"
    cp -r "$src"/. "$out/share/onlyoffice-languagetool"/
    runHook postInstall
  '';

  meta = {
    description = "LanguageTool grammar checker plugin for ONLYOFFICE Desktop Editors";
    homepage = "https://www.onlyoffice.com/marketplace/languagetool";
    license = lib.licenses.asl20;
    platforms = lib.platforms.all;
  };
}
