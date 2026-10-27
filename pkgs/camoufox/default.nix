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
  libx11,
  libxcomposite,
  libxcursor,
  libxdamage,
  libxext,
  libxfixes,
  libxi,
  libxrandr,
  libxrender,
  libxt,
  libxtst,
  libxcb,
}:
let
  version = "158.0-beta.40";
  versionParts = lib.splitString "-" version;
in
stdenv.mkDerivation {
  pname = "camoufox";
  version = version;
  src = fetchurl {
    url = "https://github.com/daijro/camoufox/releases/download/v${version}/camoufox-${version}-lin.x86_64.zip";
    hash = "sha256-Lq1JjUdlOUrd0yeNamY5YvP5mxiamdP5Zt5WZWzvyCM=";
  };
  nativeBuildInputs = [ unzip autoPatchelfHook patchelfUnstable ];
  # Firefox's relrhack loader reads relocations at fixed offsets. Preserve the
  # original sections, as nixpkgs does for firefox-bin, when adding Nix runpaths.
  patchelfFlags = [ "--no-clobber-old-sections" ];
  buildInputs = [
    alsa-lib atk cairo cups dbus dbus-glib fontconfig freetype gdk-pixbuf
    glib gtk3 libdrm libGL libxkbcommon mesa nspr nss pango
    stdenv.cc.cc.lib
    libx11 libxcomposite libxcursor libxdamage libxext libxfixes
    libxi libxrandr libxrender libxt libxtst libxcb
  ];
  unpackPhase = ''
    runHook preUnpack
    mkdir source
    # The release archive contains font names that unzip reports as local/central
    # filename mismatches. It still extracts them successfully but exits 1.
    unzip -q "$src" -d source || [ $? -eq 1 ]
    runHook postUnpack
  '';
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib/camoufox" "$out/bin"
    cp -a source/. "$out/lib/camoufox/"
    chmod +x "$out/lib/camoufox/camoufox-bin"
    printf '%s\n' '{"version":"${lib.head versionParts}","release":"${lib.last versionParts}"}' > "$out/lib/camoufox/version.json"
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
