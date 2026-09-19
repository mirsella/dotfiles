{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "rtk";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/rtk-ai/rtk";
    rev = "0924356b4caba4989607227b7c8824d3d8098719";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
}
