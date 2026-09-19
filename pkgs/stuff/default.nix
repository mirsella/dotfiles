{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "stuff";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/mirsella/stuff";
    rev = "66ea5f9d8e28a3c60b7613410c6be0431f1a87fe";
    hash = "sha256-LJfOkitjUW/RVf8DF2u71iw8fBoHTsjoj4TAhyJdhyU=";
  };
  cargoHash = "sha256-viVaFNirQB11udkifOe829D7WohmRFojHht6HojcLZI=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
