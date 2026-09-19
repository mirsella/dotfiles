{ lib, buildNpmPackage, fetchurl }:
buildNpmPackage rec {
  pname = "nodemon";
  version = "3.1.14";
  src = fetchurl {
    url = "https://registry.npmjs.org/nodemon/-/nodemon-3.1.14.tgz";
    hash = "sha256-b4hRvYDTJSZhL23WcDtlYDzvSXlEp8CBWArknOOi8lI=";
  };
  sourceRoot = "package";

  postPatch = ''
    cp ${./package-lock.json} package-lock.json
  '';

  npmDepsHash = "sha256-NY8qj/57jE9ieaBk22Lp5oSFRs+q8kgggRlztd2bxc4=";
  dontNpmBuild = true;
}
