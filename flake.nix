{
  description = "mirsella dotfiles";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nix-cachyos-kernel.url = "github:xddxdd/nix-cachyos-kernel/release";
    neovim-nightly-overlay.url = "github:nix-community/neovim-nightly-overlay";
    home-manager = {
      url = "github:nix-community/home-manager/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    lanzaboote = {
      url = "github:nix-community/lanzaboote/v1.1.0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, home-manager, sops-nix, ... }@inputs:
    let
      cachyosCache = "https://attic.xuyh0120.win/lantian";
      overlays = [
        inputs.neovim-nightly-overlay.overlays.default
        (final: _: import ./pkgs final)
      ];
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        inherit overlays;
      };
      customPackages = import ./pkgs pkgs;
      updatePackages = pkgs.writeShellApplication {
        name = "update-packages";
        runtimeInputs = [ pkgs.nix pkgs.nix-update ];
        text = ''
          if [[ ! -f flake.nix || ! -d pkgs ]]; then
            echo "Run from the dotfiles repository root" >&2
            exit 1
          fi

          nix flake update
          for package in ${nixpkgs.lib.escapeShellArgs (builtins.attrNames customPackages)}; do
            if [[ -f "pkgs/$package/package-lock.json" ]]; then
              nix-update --flake --version stable --generate-lockfile "$package"
            else
              nix-update --flake --version branch "$package"
            fi
          done
          nix flake check --no-build --no-update-lock-file

          store_paths=$(nix eval --raw --apply '
            c: builtins.concatStringsSep "\n" [
              c.boot.kernelPackages.kernel.outPath
              c.boot.kernelPackages.kernel.dev.outPath
              c.boot.zfs.package.outPath
            ]
          ' 'path:.#nixosConfigurations.predator.config')
          while IFS= read -r store_path; do
            nix path-info --store '${cachyosCache}' "$store_path" >/dev/null
          done <<< "$store_paths"
        '';
      };
      mkStandaloneHome =
        hostName: gitSigningKey:
        home-manager.lib.homeManagerConfiguration {
          inherit pkgs;
          extraSpecialArgs = {
            inherit inputs gitSigningKey hostName;
            isNixOS = false;
          };
          modules = [ ./hosts/arch.nix ];
        };
    in
    {
      packages.x86_64-linux = customPackages // { update-packages = updatePackages; };

      nixosConfigurations.predator = nixpkgs.lib.nixosSystem {
        specialArgs = { inherit inputs overlays; };
        modules = [
          {
            nix.settings.extra-substituters = [ cachyosCache ];
            nix.settings.extra-trusted-public-keys = [ "lantian:EeAUQ+W+6r7EtwnmYjeVwx5kOGEBpjlBfPlzGlTNvHc=" ];
          }
          ./configuration.nix
          home-manager.nixosModules.home-manager
          sops-nix.nixosModules.sops
        ];
      };

      homeConfigurations = {
        laptop = mkStandaloneHome "laptop" "E88ECCA3AA187BC1";
        main = mkStandaloneHome "main" "E53202A06B2614A4";
      };
    };
}
