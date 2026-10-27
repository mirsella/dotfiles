{
  lib,
  buildNpmPackage,
  fetchurl,
  git,
  nodejs,
}:
buildNpmPackage rec {
  pname = "openchamber";
  version = "2.5.2";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-5AOelqPRsJkNiMhbkYNpQ7wCO/al8pNa12RgAfEWq+4=";
  };
  sourceRoot = "package";
  patches = [ ./simple-git-4.patch ];

  postPatch = ''
    # Published assets need only runtime dependencies. Upstream still pins
    # simple-git 3, whose argument guards have several security advisories.
    ${lib.getExe nodejs} -e '
      const fs = require("node:fs");
      const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
      delete pkg.devDependencies;
      pkg.dependencies["simple-git"] = "4.0.2";
      fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
    '
    cp ${./package-lock.json} package-lock.json
  '';

  npmDepsHash = "sha256-goPSRv1buKC81PGrRDRXa0Jiba3I+RfBzYV98C72DLk=";

  npmFlags = [
    "--legacy-peer-deps"
    "--dangerously-allow-all-scripts"
    "--allow-remote=all"
  ];
  # Published assets are built; prepack refers to the upstream monorepo.
  npmPackFlags = [ "--ignore-scripts" ];

  dontNpmBuild = true;

  # GHSA-vfj7-8cjw-p6xm has no upstream release with a depth limit yet.
  postInstall = ''
    patch -p1 -d "$out/lib/node_modules/@openchamber/web/node_modules/braces" < ${./braces-depth-limit.patch}
  '';
  doInstallCheck = true;
  nativeInstallCheckInputs = [ git ];
  installCheckPhase = ''
    runHook preInstallCheck
    node ${./smoke-test.mjs} "$out/lib/node_modules/@openchamber/web"
    runHook postInstallCheck
  '';

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
