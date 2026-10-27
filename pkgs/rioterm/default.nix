{ makeRustPlatform, rust-bin, fetchgit, pkg-config, glslang, shaderc, fontconfig, freetype, libxkbcommon, wayland, libx11, libXcursor, libXi }:
let
  rust = rust-bin.stable.latest.default;
  rustPlatform = makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
in
rustPlatform.buildRustPackage {
  pname = "rioterm";
  version = "nightly-unstable-2026-10-27";
  src = fetchgit {
    url = "https://github.com/mirsella/rio";
    rev = "7973c3d10425d442175271bf0ed1f503b5558c6e";
    hash = "sha256-17ahoeIp6ZrbA/NTLMK5f3P181poHKqxJcQSIIppcwQ=";
  };
  cargoHash = "sha256-0NSo0WLWFDCjLoSoVVEysYFT2WULWmzNQ8A/9aR5mDI=";
  nativeBuildInputs = [ pkg-config glslang shaderc ];
  buildInputs = [ fontconfig freetype libxkbcommon wayland libx11 libXcursor libXi ];
  doCheck = false;
}
