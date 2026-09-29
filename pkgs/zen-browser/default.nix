{
  lib,
  stdenv,
  fetchurl,
  makeWrapper,
  makeDesktopItem,
  copyDesktopItems,
  autoPatchelfHook,
  patchelfUnstable,
  adwaita-icon-theme,
  alsa-lib,
  curl,
  dbus-glib,
  gtk3,
  libva,
  libXtst,
  pciutils,
  pipewire,
  writeText,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "zen-browser";
  version = "1.22.3b";

  src = fetchurl {
    url = "https://github.com/zen-browser/desktop/releases/download/${finalAttrs.version}/zen.linux-x86_64.tar.xz";
    hash = "sha256-Cq7hs/Z/B0rr9/xsL+gkRBvK/UMemZEzZHnBY3BRndA=";
  };

  nativeBuildInputs = [
    makeWrapper
    copyDesktopItems
    autoPatchelfHook
    patchelfUnstable
  ];

  buildInputs = [
    gtk3
    alsa-lib
    adwaita-icon-theme
    dbus-glib
    libXtst
  ];

  runtimeDependencies = [
    curl
    libva.out
    pciutils
  ];

  appendRunpaths = [ "${pipewire}/lib" ];

  patchelfFlags = [ "--no-clobber-old-sections" ];

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/zen $out/bin
    cp -a . $out/lib/zen
    makeWrapper $out/lib/zen/zen $out/bin/zen \
      --prefix LD_LIBRARY_PATH : "$out/lib/zen"
    install -Dm644 ${writeText "zen-policies.json" (builtins.toJSON {
      policies.DisableAppUpdate = true;
    })} $out/lib/zen/distribution/policies.json
    install -Dm644 $out/lib/zen/browser/chrome/icons/default/default128.png \
      $out/share/icons/hicolor/128x128/apps/zen-browser.png
    runHook postInstall
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "zen-browser";
      desktopName = "Zen Browser";
      exec = "zen %U";
      icon = "zen-browser";
      categories = [
        "Network"
        "WebBrowser"
      ];
      startupNotify = true;
      mimeTypes = [
        "text/html"
        "text/xml"
        "application/xhtml+xml"
        "x-scheme-handler/http"
        "x-scheme-handler/https"
      ];
    })
  ];

  meta = {
    description = "Privacy-focused Firefox-based browser";
    homepage = "https://zen-browser.app";
    license = lib.licenses.mpl20;
    sourceProvenance = with lib.sourceTypes; [ binaryNativeCode ];
    mainProgram = "zen";
    platforms = [ "x86_64-linux" ];
  };
})
