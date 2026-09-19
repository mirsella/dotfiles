{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/kunobi-ninja/kache";
    rev = "af30b9102f538f8ceebc974ec9dc827e2feef0d8";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
}
