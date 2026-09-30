{ lib, buildNpmPackage, fetchurl }:
# Pinned to 1.x: OpenChamber 2.x requires OpenCode 2.x, and the workstations still run OpenCode 1.x.
buildNpmPackage rec {
  pname = "openchamber";
  version = "1.24.2";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-mrefb1ENRZj3ZNlCCKRCAqUFFVW58l/6K+YSLqeJ6zQ=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
    sed -i '/"prepack"/d' package.json
  '';

  npmDepsHash = "sha256-DUD+EaH1xgNqhV5PhIhG/a/NpzJ7TWSSnAk2jEozTyc=";

  npmFlags = [ "--legacy-peer-deps" "--dangerously-allow-all-scripts" ];

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
