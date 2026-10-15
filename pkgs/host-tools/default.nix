{
  lib,
  rustPlatform,
  openssl,
  makeWrapper,
  runtimeShell,
}:
rustPlatform.buildRustPackage {
  pname = "host-tools";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./tests
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  env.HOST_TOOLS_TEST_SHELL = runtimeShell;
  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = [ openssl ];
  postInstall = ''
    makeWrapper "$out/bin/host-tools" "$out/bin/composio-mcp-headers" \
      --add-flags "composio-mcp-headers"
  '';
  meta = {
    description = "Recurring host maintenance and monitoring helpers";
    mainProgram = "host-tools";
    platforms = lib.platforms.linux;
  };
}
