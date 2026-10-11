{ lib, buildNpmPackage, fetchurl }:
# Pinned to 1.x: OpenChamber 2.x requires OpenCode 2.x, and the workstations still run OpenCode 1.x.
buildNpmPackage rec {
  pname = "openchamber";
  version = "1.24.2";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-mrefb1ENRZj3ZNlCCKRCAqUFFVW58l/6K+YSLqeJ6zQ=";
  };
  sourceRoot = "package";

  # The queue-dedupe patch gives every queued item a stable OpenCode message id
  # plus a delivery probe, so a lost ack can no longer resend the same message
  # forever. Rebase it when bumping versions while the workstations are on 1.x.
  postPatch = ''
    cp ${./package-lock.json} package-lock.json
    patch -p1 < ${./openchamber-queue-dedupe.patch}
  '';

  npmDepsHash = "sha256-DUD+EaH1xgNqhV5PhIhG/a/NpzJ7TWSSnAk2jEozTyc=";

  npmFlags = [ "--legacy-peer-deps" "--dangerously-allow-all-scripts" ];
  # Published assets are built; prepack refers to the upstream monorepo.
  npmPackFlags = [ "--ignore-scripts" ];

  dontNpmBuild = true;

  # Fail the build if the dedupe patch ever stops landing.
  postInstall = ''
    grep -q 'dropIfAlreadyDelivered' "$out/lib/node_modules/@openchamber/web/server/lib/message-queue/runtime.js"
  '';

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
