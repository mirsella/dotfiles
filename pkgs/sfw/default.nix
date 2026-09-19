{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "sfw";
  version = "2.0.6";
  src = fetchurl {
    url = "https://registry.npmjs.org/sfw/-/sfw-2.0.6.tgz";
    hash = lib.fakeHash;
  };
  npmDepsHash = lib.fakeHash;
  dontNpmBuild = true;
}
