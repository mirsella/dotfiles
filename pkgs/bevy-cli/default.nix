{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "bevy-cli";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/TheBevyFlock/bevy_cli";
    rev = "719f397cacabc69099486a88c0ec5f8356327f08";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
}
