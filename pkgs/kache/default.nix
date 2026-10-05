{ lib, rustPlatform, fetchFromGitHub, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "1.0.0";
  src = fetchFromGitHub {
    owner = "kunobi-ninja";
    repo = "kache";
    tag = "v${version}";
    hash = "sha256-Kbww7mmdUAASh1sbjdoqoosSA76EALJata+Aa8SJUnE=";
  };
  cargoHash = "sha256-/qpQMj48/wp+Ui8OWpdCdqptv1ON/xLLdslVmumZhIY=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
  meta = {
    description = "Zero-copy, content-addressed build cache for Rust and C/C++";
    homepage = "https://kunobi.ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
  };
}
