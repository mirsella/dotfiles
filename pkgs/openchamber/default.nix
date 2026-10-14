{
  lib,
  buildNpmPackage,
  fetchurl,
}:
buildNpmPackage rec {
  pname = "openchamber";
  version = "2.3.1";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-SJQ5M1y+2o4rr6+tzRNFCOyHxLiO0AxLiS3JcSIk6uo=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
  '';

  npmDepsHash = "sha256-mHG9cYbD+sLOwTu/BTtKEieYEyyv7RAqNcWHaMDUZns=";

  npmFlags = [
    "--legacy-peer-deps"
    "--dangerously-allow-all-scripts"
    "--allow-remote=all"
  ];
  # Published assets are built; prepack refers to the upstream monorepo.
  npmPackFlags = [ "--ignore-scripts" ];

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
