# Refreshing the lock (must resolve with nixpkgs nodejs, the build's npm):
#   nix store prefetch-file https://registry.npmjs.org/nodemon/-/nodemon-$ver.tgz
#   tar -xzf <store-path> -C /tmp/nd-lock && cp /tmp/nd-lock/package/package.json /tmp/nd-lock/
#   apply the same overrides as postPatch below, then in /tmp/nd-lock:
#     npm install --package-lock-only --ignore-scripts --no-audit --no-fund
#   copy package-lock.json here, put lib.fakeHash below, copy the got: hash back.
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
    # Dev-only transitive deps pinned past Dependabot's unbounded ranges:
    # mocha's minimist 0.0.8 / growl 1.9.2, glob@3's minimatch 0.3.0 (<3.1.3),
    # nyc's uuid 8.3.2 (<11.1.1). $minimatch dedupes to the direct spec
    # (an exact override would EOVERRIDE it).
    ${lib.getExe nodejs} -e "const fs=require('fs');const p=JSON.parse(fs.readFileSync('package.json','utf8'));p.overrides={'growl':'1.10.0','minimist':'1.2.8','minimatch':'\$minimatch','uuid':'11.1.1'};fs.writeFileSync('package.json',JSON.stringify(p,null,2))"
  '';

  npmDepsHash = "sha256-49GpOjw48ghf90PQ/QE8CEmk5Ur4iuloEcaMFMFPs3I=";
  dontNpmBuild = true;
}
