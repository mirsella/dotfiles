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
    hermes-agent = {
      url = "github:NousResearch/hermes-agent/865ba906c1a8d93de65839ee7af487204d42e873";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.home-manager.follows = "home-manager";
    };
  };

  outputs =
    { self, nixpkgs, home-manager, sops-nix, ... }@inputs:
    let
      overlays = [
        inputs.neovim-nightly-overlay.overlays.default
        (final: _: import ./pkgs final)
      ];
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        inherit overlays;
      };
      customPackages = import ./pkgs pkgs;
      # nix-update follows each package's own versioning: npm releases regenerate the lock, release tarballs use the stable version, and VCS packages track their branch.
      releaseTarballs = [ "helium" "kache" "zen-browser" ];
      # Keep OpenChamber on 1.x while the workstations run OpenCode 1.x; unpin both together.
      pinnedPackages = [ "openchamber" ];
      localPackages = [ "host-tools" "opencode-extensions" "opencode-idle-watchdog" ];
      updateVersionFlag = name:
        if builtins.elem name releaseTarballs then
          "--version stable"
        else if builtins.pathExists (./pkgs + "/${name}/package-lock.json") then
          "--version stable --generate-lockfile"
        else
          "--version branch";
      updateCommands = builtins.concatStringsSep "\n" (
        builtins.map
          (name: "nix-update --flake ${updateVersionFlag name} ${nixpkgs.lib.escapeShellArg name}")
          (nixpkgs.lib.subtractLists (pinnedPackages ++ localPackages) (builtins.attrNames customPackages))
      );
      updatePackages = pkgs.writeShellApplication {
        name = "update-packages";
        runtimeInputs = [ pkgs.nix pkgs.nix-update pkgs.python3 ];
        text = ''
          if [[ ! -f flake.nix || ! -d pkgs ]]; then
            echo "Run from the dotfiles repository root" >&2
            exit 1
          fi

          nix flake update
          ${updateCommands}
          python3 ${./scripts/update-immich.py}
          nix flake check --no-build --no-update-lock-file
        '';
      };
      hostDirs = nixpkgs.lib.mapAttrs (name: _: ./hosts + "/${name}") (
        nixpkgs.lib.filterAttrs (_: type: type == "directory") (builtins.readDir ./hosts)
      );
      homeModules = builtins.mapAttrs (_: directory: directory + "/home.nix") (
        nixpkgs.lib.filterAttrs
          (_: directory: builtins.pathExists (directory + "/home.nix")) hostDirs
      );
      hostModules = builtins.mapAttrs (hostName: directory: {
        imports = [
          ./modules/nixos/common.nix
          directory
          (directory + "/hardware-configuration.nix")
          home-manager.nixosModules.home-manager
          sops-nix.nixosModules.sops
          inputs.lanzaboote.nixosModules.lanzaboote
        ] ++ nixpkgs.lib.optional (hostName == "predator") inputs.hermes-agent.nixosModules.default;
        _module.args.inputs = inputs;
        networking.hostName = hostName;
        nixpkgs.overlays = overlays;
        home-manager.sharedModules = [ inputs.sops-nix.homeManagerModules.sops ];
        home-manager.users.mirsella.imports = nixpkgs.lib.optional
          (builtins.hasAttr hostName homeModules) homeModules.${hostName};
      }) hostDirs;
    in
    {
      packages.x86_64-linux = customPackages // { update-packages = updatePackages; };

      nixosModules = hostModules;
      nixosConfigurations = builtins.mapAttrs (_: module: nixpkgs.lib.nixosSystem {
        modules = [ module ];
      }) hostModules;

      homeConfigurations = builtins.mapAttrs (hostName: module:
        home-manager.lib.homeManagerConfiguration {
          inherit pkgs;
          extraSpecialArgs = {
            inherit hostName;
            isNixOS = false;
          };
          modules = [
            ./modules/home/common.nix
            module
            sops-nix.homeManagerModules.sops
            {
              targets.genericLinux.enable = true;
              targets.genericLinux.gpu.enable = false;
            }
          ];
        }
      ) homeModules;

      checks.x86_64-linux = builtins.mapAttrs (name: test:
        builtins.deepSeq (import test self)
          (pkgs.runCommand "${name}-invariants" { } ''touch "$out"'')
      ) {
        hosts = ./tests/hosts.nix;
        predator = ./tests/predator.nix;
      } // {
        data-unlock = pkgs.runCommand "data-unlock-tests" { nativeBuildInputs = [ pkgs.python3 ]; } ''
          python3 ${./tests/data-unlock.py} ${pkgs.systemd} < ${pkgs.writeText "predator-crypttab" self.nixosConfigurations.predator.config.environment.etc.crypttab.text}
          touch "$out"
        '';
        arch-maintenance = pkgs.runCommand "arch-maintenance-tests" {
          nativeBuildInputs = [ pkgs.python3 ];
          src = nixpkgs.lib.fileset.toSource {
            root = ./.;
            fileset = nixpkgs.lib.fileset.unions [
              ./tests/arch-maintenance.py
              ./arch/maintenance/apply.py
              ./arch/maintenance/cleanup-rollback.py
            ];
          };
        } ''
          PYTHONDONTWRITEBYTECODE=1 python3 "$src/tests/arch-maintenance.py"
          touch "$out"
        '';
      };
    };
}
