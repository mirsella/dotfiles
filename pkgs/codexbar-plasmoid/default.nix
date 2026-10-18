{ lib, stdenvNoCC, fetchurl }:
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "codexbar-plasmoid";
  version = "0.6.0";
  src = fetchurl {
    url = "https://github.com/psimaker/codexbar-plasmoid/archive/refs/tags/v${finalAttrs.version}.tar.gz";
    hash = "sha256-ezQ3O/ATo0kD7KPA9gFmkwO4Kj1t3y2IBEXh49Yuh5o=";
  };
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    destination="$out/share/plasma/plasmoids/com.github.psimaker.codexbar"
    mkdir -p "$destination"
    cp -r metadata.json contents "$destination/"
    runHook postInstall
  '';
  meta = {
    description = "CodexBar usage widget for KDE Plasma 6";
    homepage = "https://github.com/psimaker/codexbar-plasmoid";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
})
