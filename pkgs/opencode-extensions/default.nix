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
  version = "2.0.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./package.json
      ./pnpm-lock.yaml
      ./pnpm-workspace.yaml
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
    hash = "sha256-jwDVLt7mSBqKnxh2v4i+rnFNwFy9u7DuOTvdf5JjCOU=";
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
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/share/opencode"
    cp -r dist/. "$out/share/opencode/"
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    bun -e 'const [installed, source] = process.argv.slice(1);
    for (const file of new Bun.Glob("plugins/*.js").scanSync(installed)) {
      const plugin = await import(`''${installed}/''${file}`);
      if (Object.keys(plugin).length !== 1 || typeof plugin.default?.id !== "string" || typeof plugin.default?.setup !== "function") {
        throw new Error(`Invalid installed plugin exports: ''${file}`);
      }
    }
    for (const file of new Bun.Glob("tui-plugins/*/tui.js").scanSync(`''${source}/dist`)) {
      const plugin = await import(`''${source}/dist/''${file}`);
      if (Object.keys(plugin).length !== 1 || typeof plugin.default?.id !== "string" || typeof plugin.default?.setup !== "function") {
        throw new Error(`Invalid built TUI plugin exports: ''${file}`);
      }
    }' "$out/share/opencode" "$PWD"
    runHook postInstallCheck
  '';
  meta = {
    description = "Local OpenCode server and TUI extensions";
    platforms = lib.platforms.linux;
  };
})
