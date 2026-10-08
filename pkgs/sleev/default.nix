{ lib, stdenv, fetchurl, autoPatchelfHook, makeWrapper, openssl, zlib, libxcrypt, cacert }:
let
  version = "1.8.8";
  gateway = fetchurl {
    url = "https://storage.googleapis.com/sleeve-releases/gateway/${version}/sleeve-gateway-linux-x64.tar.gz";
    sha256 = "5b76ac6884ede511e4e0cb7f5005443433e7ae8d0ca538b898345f7e3cc25ee9";
  };
in stdenv.mkDerivation {
  pname = "sleev";
  inherit version;
  src = fetchurl {
    url = "https://storage.googleapis.com/sleeve-releases/cli/${version}/sleev-linux-x64.tar.gz";
    sha256 = "d4fcdd5f14a691c1aa5027370e2f2a1d113f3d7b927c7bb2bd9744f456d47dc9";
  };
  sourceRoot = ".";
  nativeBuildInputs = [ autoPatchelfHook makeWrapper ];
  buildInputs = [ stdenv.cc.cc.lib openssl zlib libxcrypt ];
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin" "$out/libexec/sleev"
    install -m755 sleev "$out/bin/sleev"
    tar -xzf ${gateway}
    # The vendor's one-file launcher extracts a second ELF and Python libraries.
    # Extract at build time so every runtime ELF is patched, with no writable
    # executable cache or generic-Linux loader required on the deployed host.
    patchelf --set-interpreter ${stdenv.cc.bintools.dynamicLinker} sleeve-gateway-linux-x64
    export HOME="$TMPDIR/sleev-home"
    export XDG_CACHE_HOME="$HOME/.cache"
    mkdir -p "$HOME"
    status=0
    ./sleeve-gateway-linux-x64 --version || status=$?
    if [ "$status" != 0 ]; then
      echo "Vendor extraction exited $status before the extracted ELF was patched"
    fi
    test -x "$XDG_CACHE_HOME/sleev-gateway/${version}/sleeve-gateway.bin"
    cp -a "$XDG_CACHE_HOME/sleev-gateway/${version}/." "$out/libexec/sleev/"
    runHook postInstall
  '';
  postFixup = ''
    makeWrapper "$out/libexec/sleev/sleeve-gateway.bin" "$out/bin/sleeve-gateway" \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ stdenv.cc.cc.lib openssl zlib libxcrypt ]} \
      --set-default SSL_CERT_FILE ${cacert}/etc/ssl/certs/ca-bundle.crt
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    "$out/bin/sleev" --version
    "$out/bin/sleeve-gateway" --version
    runHook postInstallCheck
  '';
  meta = {
    description = "Sleev CLI and pinned local inference gateway";
    homepage = "https://sleev.ai";
    license = lib.licenses.unfree;
    platforms = [ "x86_64-linux" ];
    mainProgram = "sleev";
  };
}
