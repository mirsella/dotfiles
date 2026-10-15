{
  lib,
  stdenvNoCC,
  fetchurl,
  nodejs,
  makeWrapper,
}:
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "chrome-devtools-mcp";
  version = "1.6.0";
  src = fetchurl {
    url = "https://registry.npmjs.org/chrome-devtools-mcp/-/chrome-devtools-mcp-${finalAttrs.version}.tgz";
    hash = "sha256-HmMsLZcUtPgrTPq077nOV1CFx1/+XpdyODEprwEsnIQ=";
  };
  nativeBuildInputs = [ makeWrapper ];
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib/chrome-devtools-mcp" "$out/bin"
    cp -r build package.json "$out/lib/chrome-devtools-mcp/"
    makeWrapper ${lib.getExe nodejs} "$out/bin/chrome-devtools-mcp" \
      --add-flags "$out/lib/chrome-devtools-mcp/build/src/bin/chrome-devtools-mcp.js"
    runHook postInstall
  '';
  meta = {
    description = "Bundled Chrome DevTools MCP server without runtime package downloads";
    mainProgram = "chrome-devtools-mcp";
    license = lib.licenses.asl20;
  };
})
