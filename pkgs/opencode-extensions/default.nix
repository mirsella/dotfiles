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
    # Load routing plugins via immutable native-package paths. Bun otherwise
    # caches the old realpath of the Home Manager plugin-directory symlink.
    mkdir "$out/share/opencode/runtime"
    for vendor in claude openai; do
      mv "$out/share/opencode/plugins/$vendor-promotional-credits.js" "$out/share/opencode/runtime/"
      mkdir "$out/share/opencode/credit-routing-$vendor"
      echo "export { default } from '../runtime/$vendor-promotional-credits.js';" \
        > "$out/share/opencode/credit-routing-$vendor/server.js"
    done
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    bun -e 'for (const file of new Bun.Glob("{plugins/*.js,credit-routing-*/server.js}").scanSync(process.argv[1])) {
      const plugin = await import(`''${process.argv[1]}/''${file}`);
      if (Object.keys(plugin).length !== 1 || typeof plugin.default?.id !== "string" || typeof plugin.default?.setup !== "function") {
        throw new Error(`Invalid plugin exports: ''${file}`);
      }
    }' "$out/share/opencode"
    runHook postInstallCheck
  '';
  meta = {
    description = "Local OpenCode server and TUI extensions";
    platforms = lib.platforms.linux;
  };
})
