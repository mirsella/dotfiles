{
  lib,
  stdenvNoCC,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm_10,
  nodejs,
  bun,
}:
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "opencode-extensions";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./package.json
      ./pnpm-lock.yaml
      ./build.mjs
      ./lib
      ./plugins
      ./tui-plugins
      ./test
    ];
  };
  nativeBuildInputs = [
    nodejs
    bun
    pnpm_10
    pnpmConfigHook
  ];
  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    pnpm = pnpm_10;
    fetcherVersion = 4;
    hash = "sha256-01Ybcnh9Dv+9Rkdg+xxA40gwutRcXbSUvbgf4pexD1w=";
  };
  buildPhase = ''
    runHook preBuild
    bun build.mjs
    runHook postBuild
  '';
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    bun test
    bun -e 'for (const file of new Bun.Glob("dist/plugins/*.js").scanSync(".")) {
      const plugin = await import(`./''${file}`);
      if (!Object.keys(plugin).length || Object.values(plugin).some(value => typeof value !== "function")) {
        throw new Error(`Invalid compiled plugin exports: ''${file}`);
      }
    }'
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/share/opencode"
    cp -r dist/plugins dist/tui-plugins "$out/share/opencode/"
    runHook postInstall
  '';
  meta = {
    description = "Local OpenCode server and TUI extensions";
    platforms = lib.platforms.linux;
  };
})
