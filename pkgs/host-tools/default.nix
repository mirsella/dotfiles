{
  lib,
  rustPlatform,
  openssl,
  makeWrapper,
  runtimeShell,
  sqlite,
  git,
  dbus,
  pkg-config,
  chezmoi,
  util-linux,
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
  env.HOST_TOOLS_TEST_DBUS_DAEMON = "${dbus}/bin/dbus-daemon";
  env.HOST_TOOLS_TEST_CHEZMOI = lib.getExe chezmoi;
  env.HOST_TOOLS_TEST_CHEZMOI_TEMPLATE = "${../../dotfiles/private_dot_config/chezmoi/chezmoi.toml.tmpl}";
  env.HOST_TOOLS_TEST_CHEZMOI_IGNORE = "${../../dotfiles/.chezmoiignore}";
  nativeBuildInputs = [
    makeWrapper
    pkg-config
  ];
  buildInputs = [ dbus ];
  nativeCheckInputs = [
    chezmoi
    openssl
    sqlite
    git
    util-linux
  ];
  postInstall = ''
    wrapProgram "$out/bin/host-tools" --prefix PATH : "${lib.makeBinPath [ sqlite git ]}"
    makeWrapper "$out/bin/host-tools" "$out/bin/composio-mcp-headers" \
      --add-flags "composio-mcp-headers"
  '';
  meta = {
    description = "Recurring host maintenance and monitoring helpers";
    mainProgram = "host-tools";
    platforms = lib.platforms.linux;
  };
}
