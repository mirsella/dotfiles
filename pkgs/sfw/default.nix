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

    # Upstream caches the firewall binary beside the package, which is read-only
    # in the Nix store. Keep its SFW_CACHE_DIR override and default to the user's cache.
    # --replace-fail keeps this loud when upstream rebundles dist/sfw.mjs.
    substituteInPlace dist/sfw.mjs \
      --replace-fail 'var NEXT_CHECK_FILE = ".sfw-cache/next-check";' "" \
      --replace-fail 'var INSTALL_ROOT = path2.resolve(__dirname2, "..");' 'var CACHE_ROOT = process.env.SFW_CACHE_DIR || path2.join(process.env.HOME, ".cache", "sfw");' \
      --replace-fail 'var NEXT_CHECK_PATH = path2.join(INSTALL_ROOT, NEXT_CHECK_FILE);' 'var NEXT_CHECK_PATH = path2.join(CACHE_ROOT, "next-check");' \
      --replace-fail 'var LATEST_SYMLINK_PATH = path2.join(INSTALL_ROOT, ".sfw-cache", "latest");' 'var LATEST_SYMLINK_PATH = path2.join(CACHE_ROOT, "latest");' \
      --replace-fail 'const cacheDir = path2.join(INSTALL_ROOT, ".sfw-cache", releaseName);' 'const cacheDir = path2.join(CACHE_ROOT, releaseName);'
  '';

  npmDepsHash = "sha256-JDXKDvJ3Y6hVMsE90SfX1N0cSRX2FNFwbwGoKMAlCGg=";
  dontNpmBuild = true;
}
