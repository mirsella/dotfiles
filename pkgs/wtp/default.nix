{ lib, buildGoModule, fetchFromGitHub }:
buildGoModule rec {
  pname = "wtp";
  version = "2.10.3-unstable-2026-09-21";

  # Fork with nushell support until https://github.com/satococoa/wtp/issues/120 is resolved.
  src = fetchFromGitHub {
    owner = "mirsella";
    repo = "wtp";
    rev = "fcffb7cdde57c938f91e1797e001e479584e7b8c";
    hash = "sha256-c/TG+YKvCKA/u5lrFGAqxgIsnxQRdDLsrr/Vya9fllQ=";
  };

  vendorHash = "sha256-zsSNo1MQgpvH3ZSd3kmvdIpOCVJgSu1/pYLltx/9dZg=";

  subPackages = [ "cmd/wtp" ];

  # Upstream integration test asserts on git CLI error wording that
  # differs in nixpkgs' git, so checks stay off for this package.
  doCheck = false;

  ldflags = [
    "-s"
    "-w"
    "-X main.version=${version}"
    "-X main.commit=${src.rev}"
    "-X main.date=${builtins.substring (builtins.stringLength version - 10) 10 version}"
  ];
}
