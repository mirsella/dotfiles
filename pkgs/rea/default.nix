{
  lib,
  stdenv,
  buildNpmPackage,
  fetchurl,
  nodejs_24,
  ghidra,
  jdk21,
  autoPatchelfHook,
}:
assert lib.assertMsg (lib.hasPrefix "12.1." ghidra.version)
  "REA's Ghidra adapter requires 12.1.x; review compatibility before upgrading";
buildNpmPackage rec {
  pname = "rea";
  version = "6.1.0";

  src = fetchurl {
    url = "https://registry.npmjs.org/rea-agents/-/rea-agents-${version}.tgz";
    hash = "sha256-+Zf2A7Px+48GWT7y1XmJ5DzdvXUmxFvOySzmHShzrTE=";
  };
  sourceRoot = "package";
  nodejs = nodejs_24;

  postPatch = ''
    # Published JS and catalogs are already compiled. Lock runtime dependencies
    # only, without the upstream development toolchain or lifecycle hooks.
    HOME="$TMPDIR" ${nodejs_24}/bin/npm pkg delete devDependencies
    cp ${./package-lock.json} package-lock.json
    ${nodejs_24}/bin/node --input-type=module <<'JS'
      import fs from "node:fs";
      const path = "skills/reverse-engineer-anything/SKILL.md";
      const source = fs.readFileSync(path, "utf8");
      const connection = /^## Connect only when needed\n[\s\S]*?(?=^## Route the target first\n)/m;
      if (!connection.test(source)) throw new Error("REA skill connection section changed; review the Nix instructions");
      fs.writeFileSync(path, source.replace(connection, `## Nix installation

    The workstation Home Manager profiles install REA, Ghidra and this matching
    skill. The OpenCode MCP server is disabled by default. Enable rea through
    /mcp when needed; restart OpenCode after package or configuration updates.
    When tools are available, proceed directly to the target using the connected
    server's actual tool list and input schemas. Do not diagnose before every task.

    If the server fails to connect, run rea mcp doctor --json. For native engine
    failures, run rea doctor --provider ghidra --json. Repair the dotfiles flake and
    rebuild; never run rea setup or npm installation to change this managed setup.

    The CLI works independently of MCP. For a JavaScript tree or ASAR, use
    rea analyze-javascript-application /absolute/path/to/app --json; no native
    engine is required. Native CLI tasks use the packaged Ghidra engine. CLI
    results require the same Evidence, limitations and unknowns review as MCP.

    `).replaceAll("npx -y rea-agents@latest", "rea"));
    JS
  '';

  npmDepsHash = "sha256-6xacTA7D3yVHhVs9388d4o7aZ+cfRWWvWdipEVLwvaQ=";
  npmFlags = [ "--ignore-scripts" ];
  dontNpmBuild = true;
  dontNpmPrune = true;

  # The process-observation adapter ships a native node-pty helper.
  nativeBuildInputs = [ autoPatchelfHook ];
  buildInputs = [ (lib.getLib stdenv.cc.cc) ];

  postInstall = ''
    for command in rea rea-agents; do
      wrapProgram "$out/bin/$command" \
        --set GHIDRA_INSTALL_DIR "${ghidra}/lib/ghidra" \
        --set JAVA_HOME "${jdk21.home}" \
        --set-default REA_ANALYSIS_PROVIDER ghidra
    done
  '';

  meta = {
    description = "Reverse engineering CLI and MCP server with Ghidra";
    homepage = "https://rea.tools/";
    license = lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
    mainProgram = "rea";
  };
}
