# Dotfiles: everyday workflow, rollback, and agent boundaries

Single repo, one `flake.lock`, three targets:

| host     | mode       | output                        | apply command                                  |
|----------|------------|-------------------------------|------------------------------------------------|
| predator | NixOS      | `nixosConfigurations.predator`| `sudo nixos-rebuild switch --flake .#predator` |
| laptop   | standalone | `homeConfigurations.laptop`   | build `activationPackage`, run `result/activate` |
| main     | standalone | `homeConfigurations.main`     | same as laptop                                 |

## Everyday loop

```nu
dots edit modules/home/files/nushell/config.nu
dots diff
dots apply
git add <changed files>; git commit -m "..."; git push   # explicit publishing only
# another machine:
dots sync
```

`dots apply` tolerates a dirty checkout but refuses untracked files (flakes
ignore them). `dots sync` requires a clean tree and fast-forwards only.
`nix flake update` is explicit maintenance: review, then build all three
targets before committing the new lock.

## Roles

- `modules/home/common.nix` — shared files, git/ssh settings, user services.
  Parameterized by `managedPackages` / `useSystemSops`; never reads the build
  hostname. Host differences arrive via `extraSpecialArgs`.
- `modules/home/server.nix` — predator-only Nix package set.
- `modules/home/workstation.nix` — Arch-only: sops-nix user secrets, no Nix
  packages, no GPU integration (`targets.genericLinux.gpu.enable = false`).
- `hosts/laptop.nix`, `hosts/main.nix` — thin per-host imports.
- Arch stays config-only: pacman/AUR own every application binary
  (`programs.git.package = null`, native `/usr/bin` in service commands).
  Never fake a derivation for a native binary.

## Secrets

Ciphertext in `secrets/` only, recipients in `.sops.yaml`. Predator uses the
NixOS sops module (host SSH key); Arch uses the Home Manager sops module
(`~/.ssh/id_ed25519`). Services order after `sops-nix.service` on Arch.
Never put plaintext in `home.file.text`, interpolation, or build inputs.
No gitleaks binary on the work machines yet — new files get a manual
secret review before commit instead.

## Rollback

- Predator: boot the previous generation, or
  `sudo nixos-rebuild switch --rollback`.
- Arch: `home-manager generations` are kept; activate an older
  `~/.local/state/nix/profiles/home-manager-*-link/activate`. HM never
  restores pacman versions or browser state — those are separate backups.

## Boundaries for future agents

1. Predator applies via `nixos-rebuild switch` only. Never activate a
   standalone HM profile for the same user there.
2. Do not push, restart production services, change disk layouts, or
   activate an unfamiliar host without explicit approval.
3. Keep storage, boot, network, Nextcloud, and backups out of dotfile
   refactors.
4. A normal apply must not repair/update the lock silently.
5. Never displace a live file with `force = true`; back up displaced files
   timestamped and get approval first.
6. Test `dots` changes against fixtures, not real profiles.
