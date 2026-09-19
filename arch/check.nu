#!/usr/bin/env nu
# Non-mutating prerequisite check for Arch standalone Home Manager targets.
# Exits nonzero with actionable diagnostics; never installs anything.

mut missing = []

if (which nix | is-empty) {
    $missing ++= ["nix binary not found; install Nix (multi-user) and enable flakes"]
} else {
    let feats = (nix --extra-experimental-features "nix-command flakes" show-config --json
        | from json | get -o experimental-features.value | default [])
    if not (["nix-command" "flakes"] | all { $in in $feats }) {
        $missing ++= ["flakes not enabled; add nix-command + flakes to nix.conf"]
    }
}

for b in [git, nu, nvim, delta, difft, mergiraf, git-lfs, atuin, starship, zoxide, opencode, lspmux, rclone, age, ssh] {
    if (which $b | is-empty) {
        $missing ++= [$"native program missing: ($b) (pacman/AUR provides it)"]
    }
}

if not (("~/.ssh/id_ed25519" | path expand) | path exists) {
    $missing ++= ["~/.ssh/id_ed25519 missing; needed as the sops age key"]
}

if ($missing | is-not-empty) {
    print ($missing | str join "\n")
    exit 1
}
print "prereqs ok"
