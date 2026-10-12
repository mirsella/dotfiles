{ lib, buildNpmPackage, fetchurl, makeWrapper }:
buildNpmPackage rec {
  pname = "sfw";
  version = "2.0.6";
  src = fetchurl {
    url = "https://registry.npmjs.org/sfw/-/sfw-${version}.tgz";
    hash = "sha256-uHGhPMeKmTuJ7Ur3E00+x+ela4YC1g9mlWn5v/SgK6Q=";
  };
  sourceRoot = "package";

  nativeBuildInputs = [ makeWrapper ];

  postPatch = ''
    cp ${./package-lock.json} package-lock.json

    # The upstream installer caches the firewall binary in INSTALL_ROOT/.sfw-cache,
    # which is read-only when installed from the Nix store. Redirect the cache to
    # SFW_CACHE_DIR (defaulting to $HOME/.cache/sfw via the wrapper below) so
    # `sfw npx`/`sfw npm`/… can download and update the binary like on Arch.
    # --replace-fail keeps this loud when upstream rebundles dist/sfw.mjs.
    substituteInPlace dist/sfw.mjs \
      --replace-fail 'var INSTALL_ROOT = path2.resolve(__dirname2, "..");' 'var INSTALL_ROOT = path2.resolve(__dirname2, "..");
var CACHE_ROOT = process.env.SFW_CACHE_DIR || path2.join(INSTALL_ROOT, ".sfw-cache");' \
      --replace-fail 'var NEXT_CHECK_PATH = path2.join(INSTALL_ROOT, NEXT_CHECK_FILE);' 'var NEXT_CHECK_PATH = path2.join(CACHE_ROOT, "next-check");' \
      --replace-fail 'var LATEST_SYMLINK_PATH = path2.join(INSTALL_ROOT, ".sfw-cache", "latest");' 'var LATEST_SYMLINK_PATH = path2.join(CACHE_ROOT, "latest");' \
      --replace-fail 'const cacheDir = path2.join(INSTALL_ROOT, ".sfw-cache", releaseName);' 'const cacheDir = path2.join(CACHE_ROOT, releaseName);'
  '';

  postFixup = ''
    # --set-default would single-quote $HOME so it never expands at runtime;
    # --run embeds the export verbatim with double quotes so $HOME resolves
    # when sfw actually runs.
    wrapProgram $out/bin/sfw --run 'export SFW_CACHE_DIR="''${SFW_CACHE_DIR:-$HOME/.cache/sfw}"'
  '';

  npmDepsHash = "sha256-JDXKDvJ3Y6hVMsE90SfX1N0cSRX2FNFwbwGoKMAlCGg=";
  dontNpmBuild = true;
}
