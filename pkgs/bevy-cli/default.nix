{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "bevy-cli";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/TheBevyFlock/bevy_cli";
    rev = "719f397cacabc69099486a88c0ec5f8356327f08";
    hash = "sha256-rEfQ5oOsNtnZoOjcIOpxundvhZeQ1HRgj9WVtAwXrf4=";
  };
  cargoHash = "sha256-gjabfj+3a4Jsre7UmwMlKgwiLWa6eClpZ/zC0uaMvfo=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
