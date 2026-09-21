# Bumping versions (on predator, has network):
#   ver=1.25.0
#   nix store prefetch-file https://registry.npmjs.org/@openchamber/web/-/web-$ver.tgz
#   tar -xzf <store-path> -C /tmp/oc-lock && cd /tmp/oc-lock/package
#   nix shell nixpkgs/nixos-26.05#nodejs --command bash -c \
#     "npm install --package-lock-only --ignore-scripts --no-audit --no-fund --legacy-peer-deps"
#   copy package-lock.json here, update version + hashes below (build errors print the right ones).
{ lib, buildNpmPackage, fetchurl }:
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
    sed -i '/"prepack"/d' package.json
  '';

  npmDepsHash = "sha256-T/rokk+gUa0UusZPpaiphP59KSxXpb13ttFICWT4Kpg=";

  npmFlags = [ "--legacy-peer-deps" "--dangerously-allow-all-scripts" ];

  dontNpmBuild = true;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
