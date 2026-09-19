{ lib, rustPlatform, fetchgit, pkg-config, fontconfig, freetype, libxkbcommon, wayland, libx11, libXcursor, libXi }:
rustPlatform.buildRustPackage rec {
  pname = "rioterm";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/mirsella/rio";
    rev = "330eb6b19cae20283619645ff55e54478bd20577";
    hash = lib.fakeHash;
  };
  cargoHash = lib.fakeHash;
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ fontconfig freetype libxkbcommon wayland libx11 libXcursor libXi ];
}
