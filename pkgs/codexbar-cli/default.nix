{
  lib,
  stdenvNoCC,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  gcc-unwrapped,
  curl,
  sqlite,
}:
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "codexbar-cli";
  version = "0.77.0";

  src = fetchurl {
    url = "https://github.com/steipete/CodexBar/releases/download/v${finalAttrs.version}/CodexBarCLI-v${finalAttrs.version}-linux-x86_64.tar.gz";
    hash = "sha256-qcMLi1bsL8hSa2O6Ry8xmJYFx+AcLGWslbSr8gt+VNQ=";
  };
  sourceRoot = ".";
  nativeBuildInputs = [ autoPatchelfHook makeWrapper ];
  buildInputs = [ gcc-unwrapped.lib curl sqlite ];
  dontBuild = true;
  dontStrip = true;

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/libexec/codexbar" "$out/bin"
    cp -r CodexBarCLI VERSION CodexBar_CodexBarCore.bundle "$out/libexec/codexbar/"
    # Resource lookup needs an absolute argv[0], with VERSION and the bundle beside it.
    makeWrapper "$out/libexec/codexbar/CodexBarCLI" "$out/bin/codexbar"
    runHook postInstall
  '';

  meta = {
    description = "CLI for AI coding provider usage quotas";
    homepage = "https://github.com/steipete/CodexBar";
    license = lib.licenses.mit;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "codexbar";
  };
})
