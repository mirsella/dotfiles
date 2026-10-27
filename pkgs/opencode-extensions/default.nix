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
    hash = "sha256-miQBs2m7sK1TzpPmm8KtCArE8lpzDiHc67FG3odONJA=";
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
    bun -e 'for (const [label, root, pattern] of [["plugin", process.argv[1], "plugins/*.js"], ["TUI plugin", `''${process.argv[2]}/dist`, "tui-plugins/*/tui.js"]]) {
    for (const file of new Bun.Glob(pattern).scanSync(root)) {
      const plugin = await import(`''${root}/''${file}`);
      if (Object.keys(plugin).length !== 1 || typeof plugin.default?.id !== "string" || typeof plugin.default?.setup !== "function") {
        throw new Error(`Invalid ''${label} exports: ''${file}`);
      }
    }
    }' "$out/share/opencode" "$PWD"
    runHook postInstallCheck
  '';
  meta = {
    description = "Local OpenCode server and TUI extensions";
    platforms = lib.platforms.linux;
  };
})
