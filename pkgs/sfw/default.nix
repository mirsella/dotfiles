{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "sfw";
  version = "2.0.6";
  src = fetchurl {
    url = "https://registry.npmjs.org/sfw/-/sfw-${version}.tgz";
    hash = "sha256-uHGhPMeKmTuJ7Ur3E00+x+ela4YC1g9mlWn5v/SgK6Q=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
  '';

  npmDepsHash = "sha256-JDXKDvJ3Y6hVMsE90SfX1N0cSRX2FNFwbwGoKMAlCGg=";
  dontNpmBuild = true;
}
