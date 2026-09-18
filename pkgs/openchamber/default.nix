{ lib, buildNpmPackage, fetchurl, jq }:
buildNpmPackage rec {
  pname = "openchamber";
  version = "1.24.1";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-T+9bVDR7y3sb46LtudzTCOMWNxLVW0tybGKug9R/YAc=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
    jq '.allowScripts = {"node-pty": true} | del(.scripts.prepack, .scripts.prepare)' package.json > package.json.tmp
    mv package.json.tmp package.json
  '';

  npmDepsHash = "sha256-T/rokk+gUa0UusZPpaiphP59KSxXpb13ttFICWT4Kpg=";

  npmFlags = [ "--legacy-peer-deps" ];

  nativeBuildInputs = [ jq ];

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
