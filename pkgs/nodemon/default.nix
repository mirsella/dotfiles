{ lib, buildNpmPackage, fetchurl, nodejs }:
buildNpmPackage rec {
  pname = "nodemon";
  version = "3.1.14";
  src = fetchurl {
    url = "https://registry.npmjs.org/nodemon/-/nodemon-3.1.14.tgz";
    hash = "sha256-b4hRvYDTJSZhL23WcDtlYDzvSXlEp8CBWArknOOi8lI=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
    # Dependabot: force patched transitive dev deps. mocha pulls minimist
    # 0.0.8 and growl 1.9.2; glob@3 pulls minimatch 0.3.0 (matches the
    # unbounded <3.1.3 range of GHSA-7r86); nyc chain pulls uuid 8.3.2
    # (matches the unbounded <11.1.1 range of GHSA-w5hq). $minimatch dedupes
    # to the direct ^10.2.1 spec (exact override would EOVERRIDE it).
    # The lock above was generated with these same overrides.
    ${lib.getExe nodejs} -e "const fs=require('fs');const p=JSON.parse(fs.readFileSync('package.json','utf8'));p.overrides={'growl':'1.10.0','minimist':'1.2.8','minimatch':'\$minimatch','uuid':'11.1.1'};fs.writeFileSync('package.json',JSON.stringify(p,null,2))"
  '';

  npmDepsHash = "sha256-49GpOjw48ghf90PQ/QE8CEmk5Ur4iuloEcaMFMFPs3I=";
  dontNpmBuild = true;
}
