{ lib, buildNpmPackage, fetchurl, nodejs_22, python3, makeWrapper, novnc }:
buildNpmPackage {
  pname = "camofox-browser";
  version = "1.18.1";
  src = fetchurl {
    url = "https://github.com/jo-inc/camofox-browser/archive/39c82094013480b373df6600d44c7f036f58356e.tar.gz";
    hash = "sha256-gWCyL8a+pNQrOcOdsZ6xeTM5Hw7vjSyMtQ2dHG6HMx8=";
  };
  nodejs = nodejs_22;
  npmDepsHash = "sha256-owKundm02Moc6KbAN5XuVyAaoaevIry5EW1pkBug/mY=";
  npmFlags = [ "--omit=dev" "--legacy-peer-deps" "--ignore-scripts" ];
  npmInstallFlags = [ "--omit=dev" ];
  npmRebuildFlags = [ "--ignore-scripts" ];
  nativeBuildInputs = [ python3 makeWrapper ];
  env.PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
  dontNpmBuild = true;
  buildPhase = ''
    runHook preBuild
    runHook postBuild
  '';
  postPatch = ''
    # The upstream lock contains unused OpenClaw development dependencies and
    # an unresolved glob override. Lock only the unchanged server dependencies.
    cp ${./package.json} package.json
    cp ${./package-lock.json} package-lock.json
    cp ${../../hosts/predator/hermes/camofox.config.json} camofox.config.json
    # This pin has no lazy-launch switch and treats TAB_INACTIVITY_MS=0 as
    # five minutes. Keep the compatibility changes small and fail on drift.
    substituteInPlace lib/config.js \
      --replace-fail 'parseInt(process.env.TAB_INACTIVITY_MS) || 300000' \
      'Number.isNaN(parseInt(process.env.TAB_INACTIVITY_MS)) ? 300000 : parseInt(process.env.TAB_INACTIVITY_MS)'
    substituteInPlace server.js \
      --replace-fail '// Pre-warm browser so first request doesn'"'"'t eat a 6-7s cold start' \
      'if (process.env.CAMOFOX_PREWARM !== "false") {' \
      --replace-fail '// Idle self-shutdown removed -- Fly manages machine lifecycle via fly.toml.' \
      '} // The deployment controller owns the cold/idle lifecycle.' \
      --replace-fail '// Per-tab inactivity reaper — close tabs idle for TAB_INACTIVITY_MS' \
      'if (TAB_INACTIVITY_MS > 0) {' \
      --replace-fail '// Orphan page reaper -- force-closes Playwright pages that survived a safePageClose' \
      '} // Registered tabs are retained until the deployment controller stops them.
      // Orphan page reaper -- force-closes Playwright pages that survived a safePageClose' \
      --replace-fail "log('warn', 'xvfb not available, falling back to headless', { error: err.message, attempt });" \
      'throw new Error("Virtual display unavailable; refusing an invisible browser: " + err.message, { cause: err });'
    substituteInPlace plugins/vnc/vnc-watcher.sh \
      --replace-fail 'NOVNC_DIR="/usr/share/novnc"' 'NOVNC_DIR="${novnc}/share/webapps/novnc"'
    # Upstream writes downloads into its private tmp beside browser profiles.
    # Put only downloads in the shared temporary directory, without copies or
    # changing upstream's session cleanup. The setgid directory grants agent
    # read access; explicit file modes override the private service umask.
    substituteInPlace lib/downloads.js \
      --replace-fail 'path.join(os.tmpdir(), `camofox-download-' \
      'path.join(process.env.CAMOFOX_DOWNLOADS_DIR || os.tmpdir(), `camofox-download-' \
      --replace-fail 'await download.saveAs(filePath);' \
      'await download.saveAs(filePath); await fs.chmod(filePath, 0o640);' \
      --replace-fail 'await fs.writeFile(filePath, body);' \
      'await fs.writeFile(filePath, body); await fs.chmod(filePath, 0o640);'
    # The native writer reports {persisted:false} instead of throwing on I/O
    # failure. Make the existing export route fail before our idle stop rather
    # than maintaining a second cookie snapshot or a custom checkpoint plugin.
    substituteInPlace plugins/persistence/index.js \
      --replace-fail "await checkpoint(userId, undefined, 'storage_export', storageState);" \
      "const saved = await checkpoint(userId, undefined, 'storage_export', storageState); if (!saved?.persisted) throw new Error('Storage persistence failed');"
    substituteInPlace plugins/vnc/index.js \
      --replace-fail "await events.emitAsync('session:storage:export'," \
      "if (!events.listenerCount('session:storage:export')) throw new Error('Storage persistence plugin unavailable'); await events.emitAsync('session:storage:export',"
    patchShebangs plugins/vnc
  '';
  postBuild = ''
    # Only this native runtime dependency needs a build. Browser installation
    # and unrelated upstream postinstall hooks never run on the target.
    npm rebuild better-sqlite3 --offline --build-from-source --ignore-scripts=false
    # The pinned Camoufox Juggler schema predates this Playwright option.
    # Storage-state restore enables interception; preserve the supported call
    # rather than dropping saved cookies/localStorage on a protocol error.
    substituteInPlace node_modules/playwright-core/lib/coreBundle.js \
      --replace-fail 'this._session.send("Network.setRequestInterception", { enabled, bypassServiceWorker })' \
      'this._session.send("Network.setRequestInterception", { enabled })'
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib/camofox-browser" "$out/bin"
    cp -r server.js lib plugins package.json camofox.config.json node_modules "$out/lib/camofox-browser/"
    # lib/cookies.js imports this shared parser even without an MCP server.
    mkdir -p "$out/lib/camofox-browser/mcp/lib"
    cp mcp/lib/cookies.mjs "$out/lib/camofox-browser/mcp/lib/"
    ${nodejs_22}/bin/node --input-type=module -e \
      'await import(process.argv[1]); await import(process.argv[2]);' \
      "$out/lib/camofox-browser/plugins/persistence/index.js" \
      "$out/lib/camofox-browser/plugins/vnc/index.js"
    makeWrapper ${nodejs_22}/bin/node "$out/bin/camofox-browser" \
      --add-flags "$out/lib/camofox-browser/server.js"
    runHook postInstall
  '';
  meta = {
    description = "Pinned Camofox server with private persistence and noVNC";
    platforms = [ "x86_64-linux" ];
    license = lib.licenses.mit;
    mainProgram = "camofox-browser";
  };
}
