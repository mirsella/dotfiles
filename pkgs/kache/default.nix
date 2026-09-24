{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "0.26.3-unstable-2026-09-24";
  src = fetchgit {
    url = "https://github.com/kunobi-ninja/kache";
    rev = "abefc0cc74467cae54681187e2688b5d8b17fe37";
    hash = "sha256-k/gutaiWhMBMcbiuHombFohiyoDMaFN9dxPsOGLplsE=";
  };
  cargoHash = "sha256-z+A8e2vPOyRKy5Ng2gYiubrrsS+q+/f4pJEgMMRtEKQ=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
