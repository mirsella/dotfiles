# thin interface over chezmoi (dotfiles) and the system builders.
# dotfiles live in <repo>/dotfiles via .chezmoiroot; chezmoi owns all app configs.
# sync engine is chezmoi itself: never reimplement managed/status/diff/re-add/forget.

def checkout [] {
  let here = ($env.CURRENT_FILE | path dirname | path join ".." | path expand)
  let from_env = ($env.NIXOS_CHECKOUT? | default "")
  if ($from_env | is-not-empty) { $from_env } else { $here }
}

def main [] { print "usage: dots (status|diff|apply|capture|forget|encrypt-add|sync|sys|doctor)" }

def "main status" [] { ^chezmoi status }

def "main diff" [--reverse] {
  if $reverse { ^chezmoi diff --reverse } else { ^chezmoi diff }
}

def "main apply" [--interactive] {
  if $interactive { ^chezmoi apply --interactive } else { ^chezmoi apply }
}

# capture local versions of already-tracked files into the repo
def "main capture" [...paths: string] {
  if ($paths | is-empty) { ^chezmoi re-add } else { ^chezmoi re-add ...$paths }
}

def "main forget" [path: string] { ^chezmoi forget $path }

def "main encrypt-add" [path: string] { ^chezmoi add --encrypt $path }

# pull repo changes and show what would apply; never auto-applies
def "main sync" [] {
  let co = checkout
  ^git -C $co pull --ff-only
  print "review, then: dots apply"
  ^chezmoi status
}

# system builders: nixos on predator, activation package elsewhere
def "main sys" [] {
  let co = checkout
  match (sys host | get hostname) {
    "predator" => { ^sudo nixos-rebuild switch --flake $"($co)#predator" }
    $h => {
      let attr = $"homeConfigurations.($h).activationPackage"
      let out = (^nix --extra-experimental-features "nix-command flakes" build $"($co)#($attr)" --print-out-paths --no-link | str trim)
      ^$out
    }
  }
}

def "main doctor" [] {
  print $"host: (sys host | get hostname)"
  ^chezmoi status
  let bins = ["git" "nu" "nvim" "delta" "difft" "mergiraf" "git-lfs" "atuin" "starship" "zoxide" "opencode" "age" "ssh"]
  let missing = ($bins | where { which $in | is-empty })
  if ($missing | is-not-empty) { print $"missing native tools: ($missing | str join ', ')"; exit 1 }
  if not ("~/.ssh/id_ed25519" | path expand | path exists) { print "missing ~/.ssh/id_ed25519 (age identity)"; exit 1 }
  print "doctor ok"
}
