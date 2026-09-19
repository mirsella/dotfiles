{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "rift-cli";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/anomalyco/rift";
    rev = "757a22cb247f9b24a849c9d6bd56f49c0ec494f8";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
}
