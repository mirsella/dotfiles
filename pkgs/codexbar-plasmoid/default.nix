{ lib, stdenvNoCC, fetchurl }:
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "codexbar-plasmoid";
  version = "0.7.0";
  src = fetchurl {
    url = "https://github.com/psimaker/codexbar-plasmoid/archive/refs/tags/v${finalAttrs.version}.tar.gz";
    hash = "sha256-GZwb0OJblh0aBtZzgs24Z7RGWvydl2mLrJNGPRigYSE=";
  };
  # Show the CLI pace verdict on every rate window, not only weekly, and keep
  # Codex's credit balance from being listed twice.
  # Tracked upstream: https://github.com/psimaker/codexbar-plasmoid/issues/76
  patches = [
    ./pace-all-windows.patch
    ./codex-credits-dedupe.patch
  ];
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
