{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "zfs-dirty-flag";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
