{ lib, rustPlatform, fetchgit, pkg-config, glslang, shaderc, fontconfig, freetype, libxkbcommon, wayland, libx11, libXcursor, libXi }:
rustPlatform.buildRustPackage rec {
  pname = "rioterm";
  version = "nightly-unstable-2026-09-20";
  src = fetchgit {
    url = "https://github.com/mirsella/rio";
    rev = "3e5d2ca2297623a491c4689ab19461ed526af5f5";
    hash = "sha256-9BBg8vC/jPv04kW4XkwY1nwquMjgGfd2+gdKpkelA7Y=";
  };
  cargoHash = "sha256-esZ41SqqPOtnyJ4z8jCP+QIQQxsIYHoqQaSGhEUcdwA=";
  nativeBuildInputs = [ pkg-config glslang shaderc ];
  buildInputs = [ fontconfig freetype libxkbcommon wayland libx11 libXcursor libXi ];
  doCheck = false;
}
