{ lib, rustPlatform, fetchgit, pkg-config, glslang, shaderc, fontconfig, freetype, libxkbcommon, wayland, libx11, libXcursor, libXi }:
rustPlatform.buildRustPackage rec {
  pname = "rioterm";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/mirsella/rio";
    rev = "330eb6b19cae20283619645ff55e54478bd20577";
    hash = "sha256-X1ZS0PojqN6oqjt0ez/ggA8sfqSZ4jy2A+bTUj8pEzc=";
  };
  cargoHash = "sha256-esZ41SqqPOtnyJ4z8jCP+QIQQxsIYHoqQaSGhEUcdwA=";
  nativeBuildInputs = [ pkg-config glslang shaderc ];
  buildInputs = [ fontconfig freetype libxkbcommon wayland libx11 libXcursor libXi ];
  doCheck = false;
}
