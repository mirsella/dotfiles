{
  lib,
  stdenvNoCC,
  rustPlatform,
  fetchFromGitHub,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm_10,
  bun,
  makeWrapper,
  writeShellScript,
  chrome-devtools-mcp,
  chromium,
  systemd,
  coreutils,
  runtimeShell,
}:
let
  native = rustPlatform.buildRustPackage {
    pname = "browser-mcp";
    version = "2.0.0";
    src = fetchFromGitHub {
      owner = "mirsella";
      repo = "browser-mcp";
      rev = "4ca51263de04c8b073216c8ae15a6d94c32fb1f9";
      hash = "sha256-8MCtB6iFwnKFWD+5y8OOiqsV4HX2MzPjPoj8PX4rOKQ=";
    };
    cargoHash = "sha256-xcdPPNRMJVPJbzVnnG2t83oiU25YiqKAuoqujUoNitQ=";
    nativeCheckInputs = [ bun ];
    postPatch = ''
      # Generated fixture scripts also need sandbox-safe shebangs.
      substituteInPlace src/main.rs \
        --replace-fail '"--browser-executable=/bin/true"' '"--browser-executable=${coreutils}/bin/true"'
      substituteInPlace tests/e2e.rs \
        --replace-fail '"/bin/true"' '"${coreutils}/bin/true"' \
        --replace-fail '#!/bin/sh' '#!${runtimeShell}' \
        --replace-fail '#!/usr/bin/env bun' '#!${lib.getExe bun}'
    '';
    preCheck = ''
      export OPENCODE_BROWSER_EXECUTABLE=${coreutils}/bin/true
    '';
  };
  # Upstream invokes its downstream with bunx's package argument. Keep that
  # protocol, but resolve the pinned, already bundled server without a download.
  downstream = writeShellScript "browser-mcp-downstream" ''
    if [[ "$1" != chrome-devtools-mcp@${chrome-devtools-mcp.version} ]]; then
      echo "Unexpected Browser MCP downstream package: $1" >&2
      exit 1
    fi
    shift
    exec ${lib.getExe chrome-devtools-mcp} "$@"
  '';
in
stdenvNoCC.mkDerivation (finalAttrs: {
  inherit (native) pname version src;
  nativeBuildInputs = [
    bun
    pnpm_10
    pnpmConfigHook
    makeWrapper
  ];
  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    pnpm = pnpm_10;
    fetcherVersion = 4;
    hash = "sha256-tzrHgW0MET8oPdwE6cCXZCwhtXQMsXIUhFJzM79CKrw=";
  };
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin"
    makeWrapper ${native}/bin/browser-mcp "$out/bin/browser-mcp" \
      --prefix PATH : ${lib.makeBinPath [ systemd ]} \
      --set OPENCODE_BROWSER_DOWNSTREAM_COMMAND ${downstream} \
      --set-default OPENCODE_BROWSER_EXECUTABLE ${lib.getExe chromium}
    plugin="$out/share/browser-mcp"
    mkdir -p "$plugin/plugin" "$plugin/vendor"
    cp -r .agents .codex-plugin .mcp.json hooks skills package.json server.ts "$plugin/"
    bun build plugin/browser-routing.ts --target=bun --outfile="$plugin/plugin/browser-routing.ts"
    # Codex copies regular files but omits symlinked executables.
    cp "$out/bin/browser-mcp" "$plugin/vendor/browser-mcp"
    runHook postInstall
  '';
  meta = {
    description = "Shared browser MCP with OpenCode and Codex integrations";
    mainProgram = "browser-mcp";
    license = lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
  };
})
