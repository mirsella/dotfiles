{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "t3-idle-watchdog";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  meta = {
    description = "Run a command when the other T3 Code threads are idle";
    mainProgram = "t3-idle-watchdog";
    platforms = lib.platforms.linux;
  };
}
