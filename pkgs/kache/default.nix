{ lib, rustPlatform, fetchFromGitHub, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "1.2.2";
  src = fetchFromGitHub {
    owner = "kunobi-ninja";
    repo = "kache";
    tag = "v${version}";
    hash = "sha256-thzKe50UE6CKYYfKNrVyNdgQdAqr0bLRYOZzvWwlxW8=";
  };
  cargoHash = "sha256-wgBi4CqPgab15O6y0n0re2aZ5QV02LbIlktQVlD8rVQ=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
  meta = {
    description = "Zero-copy, content-addressed build cache for Rust and C/C++";
    homepage = "https://kunobi.ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
  };
}
