final:
let
  entries = builtins.readDir ./.;
  names = builtins.filter (
    name: entries.${name} == "directory" && builtins.pathExists (./. + "/${name}/default.nix")
  ) (builtins.attrNames entries);
in
builtins.listToAttrs (map (name: {
  inherit name;
  value = final.callPackage (./. + "/${name}") { };
}) names)
