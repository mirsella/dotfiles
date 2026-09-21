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
    # Dependabot: force patched transitive dev deps (mocha pulls minimist
    # 0.0.8 and growl 1.9.2); the lock above was generated with these overrides.
    ${lib.getExe nodejs} -e "const fs=require('fs');const p=JSON.parse(fs.readFileSync('package.json','utf8'));p.overrides={'growl':'1.10.0','minimist':'1.2.8'};fs.writeFileSync('package.json',JSON.stringify(p,null,2))"
  '';

  npmDepsHash = "sha256-hAKt2oIFOen0IJyRvJzMrMZ4oBhii+qu+Tm27d/NaZw=";
  dontNpmBuild = true;
}
