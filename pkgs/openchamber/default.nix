{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "openchamber";
  version = "1.24.1";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-T+9bVDR7y3sb46LtudzTCOMWNxLVW0tybGKug9R/YAc=";
  };
  sourceRoot = "source/package";

  npmDepsHash = lib.fakeHash;

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
