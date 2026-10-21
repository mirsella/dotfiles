{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "idle-watchdog";
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
    description = "Run a command when other OpenCode sessions or T3 Code threads are idle";
    mainProgram = "opencode-idle-watchdog";
    platforms = lib.platforms.linux;
  };
}
