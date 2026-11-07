{ lib, stdenv, fetchurl, dpkg, autoPatchelfHook, makeWrapper, symlinkJoin
, alsa-lib, alsa-plugins, openssl, libxkbcommon, vulkan-loader, openblas
, wayland, fontconfig, freetype, pipewire, libva, libx11, libxcb, libxcursor
, libxrandr, libxi, mesa, curl, wl-clipboard, wtype, ffmpeg-headless
}:
let
  runtimeLibraries = [
    vulkan-loader openblas alsa-lib wayland libxkbcommon fontconfig freetype
    pipewire libva libx11 libxcb libxcursor libxrandr libxi
  ];
  alsaPlugins = symlinkJoin {
    name = "hex-alsa-plugins";
    paths = [ "${pipewire}/lib/alsa-lib" "${alsa-plugins}/lib/alsa-lib" ];
  };
in
stdenv.mkDerivation rec {
  pname = "hex";
  version = "3.3.0";
  src = fetchurl {
    url = "https://github.com/anomalyco/hex/releases/download/v${version}/hex-${version}-linux-x64.deb";
    hash = "sha256-5bNnju0zygfIdxUI9Y34ajRva3EyQ2buk9IF6KbVRFA=";
  };

  nativeBuildInputs = [ dpkg autoPatchelfHook makeWrapper ];
  buildInputs = [ stdenv.cc.cc.lib alsa-lib openssl libxkbcommon vulkan-loader ];
  dontConfigure = true;
  dontBuild = true;

  unpackPhase = ''
    runHook preUnpack
    dpkg-deb -x "$src" .
    runHook postUnpack
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    cp -a usr/bin usr/share "$out/"
    substituteInPlace "$out/share/applications/anomaly.hex.desktop" \
      --replace-fail '/usr/bin/hex' "$out/bin/hex"
    runHook postInstall
  '';

  postFixup = ''
    wrapProgram "$out/bin/hex" \
      --prefix PATH : ${lib.makeBinPath [ curl wl-clipboard wtype ffmpeg-headless ]} \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibraries} \
      --set ALSA_PLUGIN_DIR ${alsaPlugins} \
      --set-default VK_DRIVER_FILES "${mesa}/share/vulkan/icd.d"
  '';

  meta = {
    description = "Local-first voice dictation";
    homepage = "https://anoma.ly/hex/";
    license = lib.licenses.mit;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    mainProgram = "hex";
    platforms = [ "x86_64-linux" ];
  };
}
