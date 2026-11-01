{ rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "predator-pm-study";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
