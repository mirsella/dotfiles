{
  lib,
  stdenvNoCC,
  rustPlatform,
  fetchFromGitHub,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm_10,
  bun,
  pkg-config,
  makeWrapper,
  glib,
  pipewire,
  libxkbcommon,
  bash,
  coreutils,
  procps,
  util-linux,
  dbus,
  kdePackages,
  wireplumber,
  at-spi2-core,
  xdg-desktop-portal,
  python3,
  writeShellScript,
  mesa,
}:
let
  native = rustPlatform.buildRustPackage {
    pname = "computer-use-mcp";
    version = "2.0.0";
    src = fetchFromGitHub {
      owner = "mirsella";
      repo = "computer-use-mcp";
      rev = "d129a7efd5054cf7f9f4be6355057d42d78b9bdf";
      hash = "sha256-J4O5k3UD5JY12X3b5jclC3IIVAUt1f6oSD4m6Xiuyp4=";
    };
    cargoHash = "sha256-8c67eXjrovk6uKxHjYOd28yC7Is1YP7qGJb/W4Ms+KA=";
    nativeBuildInputs = [
      pkg-config
      rustPlatform.bindgenHook
    ];
    buildInputs = [
      glib
      pipewire
      libxkbcommon
    ];
    nativeCheckInputs = [
      python3
      procps
      util-linux
    ];
    postPatch = ''
      patchShebangs crates/computer-use-mcp/tests/fixtures
    '';
  };
  # Arch does not have NixOS's /etc/dbus-1/session.conf.
  sessionBus = writeShellScript "computer-use-session-bus" ''
    if [ "''${1-}" != --session ]; then
      echo "Unexpected private D-Bus invocation: expected --session" >&2
      exit 2
    fi
    shift
    exec ${dbus}/bin/dbus-daemon --config-file=${dbus}/share/dbus-1/session.conf "$@"
  '';
  # Only the owned virtual compositor uses software EGL. The physical desktop
  # keeps its host graphics stack; Arch has no /run/opengl-driver Nix closure.
  compositor = writeShellScript "computer-use-private-compositor" ''
    export LIBGL_DRIVERS_PATH=${mesa}/lib/dri
    export GBM_BACKENDS_PATH=${mesa}/lib/gbm
    export __EGL_VENDOR_LIBRARY_FILENAMES=${mesa}/share/glvnd/egl_vendor.d/50_mesa.json
    export LIBGL_ALWAYS_SOFTWARE=1
    export KWIN_COMPOSE=O2
    exec ${kdePackages.kwin}/bin/kwin_wayland "$@"
  '';
in
stdenvNoCC.mkDerivation (finalAttrs: {
  inherit (native) pname version src;
  nativeBuildInputs = [
    bun
    pnpm_10
    pnpmConfigHook
    makeWrapper
  ];
  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    pnpm = pnpm_10;
    fetcherVersion = 4;
    hash = "sha256-H7hat5eMWb1qnQUXhdXcnsi7AFFH84xB3Re0TvHGeiE=";
  };
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin"
    makeWrapper ${native}/bin/computer-use-mcp "$out/bin/computer-use-mcp" \
      --prefix PATH : ${
        lib.makeBinPath [
          bash
          coreutils
          procps
          util-linux
          dbus
          glib
          pipewire
          wireplumber
        ]
      } \
      --set-default XDG_DATA_DIRS /usr/local/share:/usr/share \
      --prefix XDG_DATA_DIRS : ${
        lib.makeSearchPath "share" [
          xdg-desktop-portal
          kdePackages.xdg-desktop-portal-kde
          at-spi2-core
        ]
      } \
      --set-default COMPUTER_USE_MCP_DBUS_DAEMON_BIN ${sessionBus} \
      --set-default COMPUTER_USE_MCP_COMPOSITOR_BIN ${compositor} \
      --set-default COMPUTER_USE_MCP_PORTAL_BIN ${xdg-desktop-portal}/libexec/xdg-desktop-portal \
      --set-default COMPUTER_USE_MCP_PORTAL_BACKEND_BIN ${kdePackages.xdg-desktop-portal-kde}/libexec/xdg-desktop-portal-kde \
      --set-default COMPUTER_USE_MCP_ATSPI_BUS_BIN ${at-spi2-core}/libexec/at-spi-bus-launcher \
      --set-default COMPUTER_USE_MCP_ATSPI_REGISTRY_BIN ${at-spi2-core}/libexec/at-spi2-registryd
    plugin="$out/share/computer-use-mcp"
    mkdir -p "$plugin/.agents" "$plugin/plugin" "$plugin/vendor/bin" "$plugin/skills/computer-use-mcp"
    cp -r .agents/plugins "$plugin/.agents/"
    cp -r .codex-plugin .mcp.json package.json server.mjs "$plugin/"
    cp crates/computer-use-mcp/guidance/skill.md "$plugin/skills/computer-use-mcp/SKILL.md"
    bun build plugin/computer-use.mjs --target=bun --outfile="$plugin/plugin/computer-use.mjs"
    # Codex copies regular files but omits symlinked executables.
    cp "$out/bin/computer-use-mcp" "$plugin/vendor/bin/computer-use-mcp"
    runHook postInstall
  '';
  meta = {
    description = "Wayland desktop MCP with OpenCode and Codex integrations";
    mainProgram = "computer-use-mcp";
    license = lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
  };
})
