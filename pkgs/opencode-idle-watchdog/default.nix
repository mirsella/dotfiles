{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "opencode-idle-watchdog";
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
    description = "Run a command when the other OpenCode sessions are idle";
    mainProgram = "opencode-idle-watchdog";
    platforms = lib.platforms.linux;
  };
}
