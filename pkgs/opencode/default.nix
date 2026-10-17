{
  lib,
  stdenvNoCC,
  fetchurl,
  autoPatchelfHook,
  gcc-unwrapped,
}:
stdenvNoCC.mkDerivation rec {
  pname = "opencode";
  version = "2.0.28";
  src = fetchurl {
    url = "https://registry.npmjs.org/@opencode/cli-linux-x64-baseline/-/cli-linux-x64-baseline-${version}.tgz";
    hash = "sha256-QyXzTqiOCR+w34GDwhIMDRPvt0PCt/kLfK6usUfqmGI=";
  };
  sourceRoot = "package";
  nativeBuildInputs = [ autoPatchelfHook ];
  buildInputs = [ gcc-unwrapped.lib ];
  installPhase = ''
    runHook preInstall
    install -Dm755 bin/opencode $out/bin/opencode
    runHook postInstall
  '';
  meta = {
    description = "OpenCode v2 CLI and server";
    homepage = "https://opencode.ai/v2";
    license = lib.licenses.mit;
    mainProgram = "opencode";
    platforms = [ "x86_64-linux" ];
  };
}
