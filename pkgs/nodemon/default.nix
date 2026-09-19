{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "nodemon";
  version = "3.1.14";
  src = fetchurl {
    url = "https://registry.npmjs.org/nodemon/-/nodemon-3.1.14.tgz";
    hash = lib.fakeHash;
  };
  npmDepsHash = lib.fakeHash;
  dontNpmBuild = true;
}
