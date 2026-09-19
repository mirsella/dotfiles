{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/kunobi-ninja/kache";
    rev = "af30b9102f538f8ceebc974ec9dc827e2feef0d8";
    hash = "sha256-6Nu3f8noKZbrxV6+2ozQhHetl4uWWhIXfgfCeY7v1qU=";
  };
  cargoHash = "sha256-KPi6H1YuaDDK0JEYWEJOGi4HIaD32w2VGRarYql7HBA=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
