{ lib, rustPlatform, fetchgit, pkg-config, wayland, libxkbcommon }:
rustPlatform.buildRustPackage rec {
  pname = "computer-use-mcp";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/mirsella/computer-use-mcp";
    rev = "5fc109264bb8fb75ed5fa36bcacb0d08c9026ace";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ wayland libxkbcommon ];
}
