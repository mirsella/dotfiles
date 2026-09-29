{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  patchelfUnstable,
  makeWrapper,
  makeDesktopItem,
  copyDesktopItems,
  alsa-lib,
  at-spi2-core,
  cairo,
  cups,
  dbus,
  expat,
  fontconfig,
  freetype,
  gdk-pixbuf,
  glib,
  gtk3,
  libGL,
  libcap,
  libdrm,
  libgcrypt,
  libkrb5,
  libnotify,
  libpulseaudio,
  libusb1,
  libva,
  libx11,
  libxcb,
  libxcomposite,
  libxcursor,
  libxdamage,
  libxext,
  libxfixes,
  libxi,
  libxkbcommon,
  libxrandr,
  libxrender,
  libxscrnsaver,
  libxshmfence,
  libxtst,
  mesa,
  nspr,
  nss,
  pango,
  pipewire,
  qt6,
  systemd,
  util-linux,
  wayland,
  zlib,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "helium";
  version = "0.18.1.1";

  src = fetchurl {
    url = "https://github.com/imputnet/helium-linux/releases/download/${finalAttrs.version}/helium-${finalAttrs.version}-x86_64_linux.tar.xz";
    hash = "sha256-n001I57qGLKQhGIhh0JlrCqGN63/lU32n973fWsVBCw=";
  };

  nativeBuildInputs = [
    makeWrapper
    copyDesktopItems
    autoPatchelfHook
    patchelfUnstable
  ];

  buildInputs = [
    stdenv.cc.cc.lib
    alsa-lib
    at-spi2-core
    cairo
    cups
    dbus
    expat
    fontconfig
    freetype
    gdk-pixbuf
    glib
    gtk3
    libGL
    libcap
    libdrm
    libgcrypt
    libkrb5
    libnotify
    libpulseaudio
    libusb1
    libva
    libx11
    libxcb
    libxcomposite
    libxcursor
    libxdamage
    libxext
    libxfixes
    libxi
    libxkbcommon
    libxrandr
    libxrender
    libxscrnsaver
    libxshmfence
    libxtst
    mesa
    nspr
    nss
    pango
    pipewire
    qt6.qtbase
    systemd
    util-linux
    wayland
    zlib
  ];

  dontConfigure = true;
  dontBuild = true;
  dontWrapQtApps = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/helium $out/bin
    cp -a . $out/lib/helium
    rm $out/lib/helium/helium-wrapper $out/lib/helium/libqt5_shim.so
    makeWrapper $out/lib/helium/helium $out/bin/helium \
      --prefix LD_LIBRARY_PATH : "$out/lib/helium" \
      --set CHROME_WRAPPER $out/bin/helium
    install -Dm644 $out/lib/helium/product_logo_256.png \
      $out/share/icons/hicolor/256x256/apps/helium.png
    runHook postInstall
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "helium";
      desktopName = "Helium";
      exec = "helium %U";
      icon = "helium";
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
    description = "Private, fast, and honest web browser based on Chromium";
    homepage = "https://github.com/imputnet/helium-linux";
    license = lib.licenses.gpl3Only;
    sourceProvenance = with lib.sourceTypes; [ binaryNativeCode ];
    mainProgram = "helium";
    platforms = [ "x86_64-linux" ];
  };
})
