{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "host-tools";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  meta = {
    description = "Recurring host maintenance and monitoring helpers";
    mainProgram = "host-tools";
    platforms = lib.platforms.linux;
  };
}
