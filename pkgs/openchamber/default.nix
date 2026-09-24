{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "openchamber";
  version = "2.0.0";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-aER6fbGWOSJd0TzHqotuBU3CoS9OIzYEr3PX8qpMREs=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
    sed -i '/"prepack"/d' package.json
  '';

  npmDepsHash = "sha256-zTa4N2Dc5SD2vMmu+Eaxa1wabza8TZ98oBzDh8lCvmo=";

  npmFlags = [ "--legacy-peer-deps" "--dangerously-allow-all-scripts" ];

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
