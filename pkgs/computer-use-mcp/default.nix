{ lib, rustPlatform, fetchgit, pkg-config, wayland, libxkbcommon, glib, pipewire }:
rustPlatform.buildRustPackage rec {
  pname = "computer-use-mcp";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/mirsella/computer-use-mcp";
    rev = "5fc109264bb8fb75ed5fa36bcacb0d08c9026ace";
    hash = "sha256-KzJgrBMb+RqeVSDLezFOhWoatKn3RtmS8YXO2PUrG6I=";
  };
  cargoHash = "sha256-+e4qxWg9WoeX9gxKlDlHr9zrzl5X2aqP4OqSjtz6GV4=";
  nativeBuildInputs = [ pkg-config rustPlatform.bindgenHook ];
  buildInputs = [ wayland libxkbcommon glib pipewire ];
  doCheck = false;
}
