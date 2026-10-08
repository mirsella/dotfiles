{
  lib,
  stdenv,
  fetchurl,
  unzip,
  autoPatchelfHook,
  patchelfUnstable,
  alsa-lib,
  atk,
  cairo,
  cups,
  dbus,
  dbus-glib,
  fontconfig,
  freetype,
  gdk-pixbuf,
  glib,
  gtk3,
  libdrm,
  libGL,
  libxkbcommon,
  mesa,
  nspr,
  nss,
  pango,
  xorg,
}:
stdenv.mkDerivation {
  pname = "camoufox";
  version = "152.0.4-beta.28";
  src = fetchurl {
    url = "https://github.com/daijro/camoufox/releases/download/v152.0.4-beta.28/camoufox-152.0.4-beta.28-lin.x86_64.zip";
    hash = "sha256-kk8xCczW1HzWoDhNZ6NF+t+XXUi2MZ+Nu9WVTFiJgr0=";
  };
  nativeBuildInputs = [ unzip autoPatchelfHook patchelfUnstable ];
  # Firefox's relrhack loader reads relocations at fixed offsets. Preserve the
  # original sections, as nixpkgs does for firefox-bin, when adding Nix runpaths.
  patchelfFlags = [ "--no-clobber-old-sections" ];
  buildInputs = [
    alsa-lib atk cairo cups dbus dbus-glib fontconfig freetype gdk-pixbuf
    glib gtk3 libdrm libGL libxkbcommon mesa nspr nss pango
    stdenv.cc.cc.lib
    xorg.libX11 xorg.libXcomposite xorg.libXcursor xorg.libXdamage
    xorg.libXext xorg.libXfixes xorg.libXi xorg.libXrandr xorg.libXrender
    xorg.libXt xorg.libXtst xorg.libxcb
  ];
  unpackPhase = ''
    runHook preUnpack
    mkdir source
    unzip -q "$src" -d source
    runHook postUnpack
  '';
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib/camoufox" "$out/bin"
    cp -a source/. "$out/lib/camoufox/"
    chmod +x "$out/lib/camoufox/camoufox-bin"
    printf '%s\n' '{"version":"152.0.4","release":"beta.28"}' > "$out/lib/camoufox/version.json"
    ln -s "$out/lib/camoufox/camoufox-bin" "$out/bin/camoufox"
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    timeout 20 "$out/bin/camoufox" --version
    runHook postInstallCheck
  '';
  meta = {
    description = "Pinned Camoufox engine for Predator's private browser";
    platforms = [ "x86_64-linux" ];
    license = lib.licenses.mpl20;
  };
}
