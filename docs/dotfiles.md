# Dotfiles: everyday workflow, rollback, and agent boundaries

Single repo, one `flake.lock`, three targets:

| host     | mode       | output                        | apply command                                  |
|----------|------------|-------------------------------|------------------------------------------------|
| predator | NixOS      | `nixosConfigurations.predator`| `sudo nixos-rebuild switch --flake .#predator` |
| laptop   | standalone | `homeConfigurations.laptop`   | build `activationPackage`, run `result/activate` |
| main     | standalone | `homeConfigurations.main`     | same as laptop                                 |

## Everyday loop

```sh
chezmoi edit ~/.config/nvim/lua/plugins/example.lua
chezmoi diff                  # --reverse previews the opposite direction
chezmoi re-add                # local versions -> repo
chezmoi apply                 # repo -> this machine (--interactive for decisions)
chezmoi forget <path>         # stop tracking, keep the live file
git add <changed files>; git commit -m "..."; git push   # explicit publishing only
# another machine:
git pull --ff-only             # then review
chezmoi status                 # then apply explicitly
```

`dots sync` never auto-applies. `nix flake update` is explicit maintenance:
review, then build all three targets before committing the new lock.

## Ownership

chezmoi owns every editable app config (`dotfiles/`, via `.chezmoiroot`).
Home Manager must not declare files inside chezmoi-owned directories or
replace one with a symlink. HM keeps: package sets, `programs.git` /
`programs.ssh` settings (generated files), user services, secret wiring.

## Roles

- `modules/home/common.nix` — user services, git/ssh settings, rustup
  activation. Parameterized by `isNixOS`; never reads the build hostname.
  Host differences arrive via `extraSpecialArgs`.
- `modules/home/server.nix` — predator-only Nix package set.
- `modules/home/workstation.nix` — Arch-only: sops-nix user secrets, no Nix
  packages, no GPU integration (`targets.genericLinux.gpu.enable = false`).
- `hosts/arch.nix` uses shared Arch imports. The flake passes the hostname
  and signing key; the hostname selects the local inference model.
- `modules/user-secrets.nix` defines shared SOPS key names and user paths.
  NixOS and Home Manager apply their own ownership and decryption settings.
- Arch stays config-only: pacman/AUR own every application binary
  (`programs.git.package = null`, native `/usr/bin` in service commands).
  Never fake a derivation for a native binary.
- A NixOS rollback does not roll back chezmoi-owned files; their recovery
  is git history plus a reviewed `chezmoi apply`.

## Secrets

Ciphertext in `secrets/` only, recipients in `.sops.yaml`. Predator uses the
NixOS sops module (host SSH key); Arch uses the Home Manager sops module
(`~/.ssh/id_ed25519`). Services order after `sops-nix.service` on Arch.
Never put plaintext in interpolation or build inputs. Gitleaks scans the repo
(`gitleaks detect --source .`); keep it clean before every commit.

## Rollback

For an OS reinstall, disk recovery, or TPM/Secure Boot enrollment, use
[Predator reinstall and recovery](predator-reinstall.md).

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
6. Never script around chezmoi's managed/status/diff/re-add/forget commands;
   call its CLI instead of reimplementing the sync engine.
