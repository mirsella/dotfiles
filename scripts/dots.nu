#!/usr/bin/env nu
# dots — thin everyday wrapper around git + home-manager / nixos-rebuild.
# Contract: edit | diff | apply | sync | re-add | doctor. Never pushes,
# never updates the lock, never activates an unknown host.

const HOST_OUTPUTS = {predator: "nixos", laptop: "home", main: "home"}

def checkout []: string {
    let env_hit = ($env.DOTS_CHECKOUT? | default "")
    let candidates = [$env_hit $"($env.HOME)/dev/nixos"]
    for c in $candidates {
        if ($c | is-not-empty) and ([$c "flake.nix"] | path join | path exists) {
            return $c
        }
    }
    error make {msg: "no checkout found; set $env.DOTS_CHECKOUT to the repo path"}
}

def target [] {
    let host = (sys host | get hostname)
    let kind = ($HOST_OUTPUTS | get -o $host)
    if $kind == null {
        error make {msg: $"unknown host '($host)'; known: (($HOST_OUTPUTS | columns) | str join ', ')"}
    }
    {host: $host, kind: $kind}
}

def dirty [] {
    let root = (checkout)
    git -C $root status --porcelain | lines | where ($it | is-not-empty)
}

def untracked-inputs [] {
    let root = (checkout)
    dirty | where ($it | str starts-with "??") | each { $in | str substring 3.. }
}

def main [] { print "usage: dots (edit|diff|apply|sync|re-add|doctor)" }

# Open the repository source for a managed file. Never follows /nix/store links.
def "main edit" [path: string] {
    let root = (checkout)
    let generated = {
        "~/.gitconfig": "modules/home/common.nix (programs.git.settings)"
        "~/.ssh/config": "modules/home/common.nix (programs.ssh.settings)"
        "~/.config/nushell": "modules/home/files/nushell/"
        "~/.config/nvim": "modules/home/files/nvim/"
    }
    if ($path | str starts-with "/nix/store") {
        error make {msg: "refusing to edit a store path; use `dots edit <repo-source>`"}
    }
    let hit = ($generated | get -o $path)
    if $hit != null {
        print $"generated file; edit its owner instead: ($hit)"
        return
    }
    let full = ([$root $path] | path join)
    if not ($full | path exists) {
        error make {msg: $"not in repo: ($path)"}
    }
    let editor = ($env.EDITOR? | default "nvim")
    ^$editor $full
}

# Build the selected target without activating; show dirty files and drift.
def "main diff" [] {
    let t = (target)
    let root = (checkout)
    let d = (dirty)
    if ($d | is-not-empty) {
        print "dirty checkout:"
        print ($d | str join "\n")
    }
    if $t.kind == "nixos" {
        sudo nixos-rebuild build --flake $"($root)#predator"
        print "predator toplevel builds; activate with `dots apply`"
    } else {
        nix build $"($root)#homeConfigurations.($t.host).activationPackage" --no-link --print-out-paths
        print $"($t.host) activation package builds; activate with `dots apply`"
    }
}

# Build, run preflight checks, then activate the correct target for this host.
def "main apply" [] {
    let t = (target)
    let root = (checkout)
    let new_files = (untracked-inputs)
    if ($new_files | is-not-empty) {
        error make {msg: $"untracked files are invisible to flakes; git add first:\n($new_files | str join '\n')"}
    }
    let d = (dirty)
    if ($d | is-not-empty) { print "applying with dirty checkout (uncommitted edits):" }
    if $t.kind == "nixos" {
        sudo nixos-rebuild switch --flake $"($root)#predator"
    } else {
        nu ([$root "arch" "check.nu"] | path join)
        let out = (nix build $"($root)#homeConfigurations.($t.host).activationPackage" --no-link --print-out-paths | str trim)
        ^$"($out)/activate"
    }
}

# Require a clean tree, fast-forward only, show incoming, then apply.
def "main sync" [] {
    let root = (checkout)
    let d = (dirty)
    if ($d | is-not-empty) {
        error make {msg: $"clean tree required:\n($d | str join '\n')"}
    }
    git -C $root fetch origin
    let incoming = (git -C $root log --oneline $"HEAD..@{u}" | str trim)
    if ($incoming | is-empty) {
        print "already up to date"
        return
    }
    print $"incoming:\n($incoming)"
    git -C $root merge --ff-only
    main apply
}

# Explicitly import one allowlisted raw live file back into the repo.
def "main re-add" [live: string] {
    let root = (checkout)
    let home = $env.HOME
    let mapping = {
        $"($home)/.config/nushell/config.nu": "modules/home/files/nushell/config.nu"
        $"($home)/.config/nushell/env.nu": "modules/home/files/nushell/env.nu"
        $"($home)/.config/nushell/alias.nu": "modules/home/files/nushell/alias.nu"
        $"($home)/.config/nushell/functions.nu": "modules/home/files/nushell/functions.nu"
        $"($home)/.config/starship.toml": "modules/home/files/starship.toml"
    }
    let secrets = ["server.env" "telegram.env" "55-secrets.conf" "accounts.json" "tokens.json" "gcp-oauth" "config.toml"]
    if ($secrets | any { $live | str contains $in }) {
        error make {msg: "refusing: looks like a secret; manage it via sops instead"}
    }
    let dest = ($mapping | get -o $live)
    if $dest == null {
        error make {msg: $"no allowlisted mapping for ($live); edit the repo source directly"}
    }
    let repo_file = ([$root $dest] | path join)
    let patch = (^diff -u $repo_file $live | str trim)
    if ($patch | is-empty) {
        print "live file matches repo source; nothing to import"
        return
    }
    print $patch
    if (input "import into repo? (yes/no): " | str trim | str downcase) != "yes" {
        print "aborted"
        return
    }
    cp $live $repo_file
    print $"imported; review with git diff, then commit"
}

# Report target, providers, missing prereqs, failed services, secret readiness.
def "main doctor" [] {
    let t = (target)
    let root = (checkout)
    print $"host: ($t.host)  kind: ($t.kind)  checkout: ($root)"
    print $"git: (^git --version)  nu: (version | get version)"
    if $t.kind == "home" {
        nu ([$root "arch" "check.nu"] | path join)
        for b in [git, nu, nvim, opencode, atuin, starship, zoxide] {
            print $"($b): (which $b | get -o 0.path | default 'MISSING')"
        }
    } else {
        print $"system: (uname -r)"
    }
    let failed = (systemctl --user is-failed 2>/dev/null | str trim)
    if ($failed | is-not-empty) { print $"failed user services:\n($failed)" }
    for s in ["~/.config/telegram.env" "~/.config/opencode/server.env"] {
        let p = ($s | path expand)
        print $"secret ($s): (if ($p | path exists) { 'present' } else { 'MISSING' })"
    }
}
