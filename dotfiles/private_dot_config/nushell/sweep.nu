# `sweep` as a Nushell table of agent workspaces: when each goes (`due`), or
# why it stays (`state`, `reason`). Sort and filter it like any table:
#   sweep | sort-by size --reverse
#   sweep | where state == blocked | select path reason
#   sweep | where due < (date now) + 1day | get size | math sum
# Measuring sizes takes a minute or two, so keep a copy: `let w = (sweep)`.
# Any arguments run the CLI itself: `sweep clean 1h`, `sweep keep PATH`.
def --wrapped sweep [...args] {
  # Home Manager installs the CLI; shells started outside the desktop session,
  # such as T3 terminals, may not have its profile on PATH.
  let cli = (
    which --all sweep
    | where type == external
    | get path
    | append [
      $"($env.HOME)/.nix-profile/bin/sweep"
      $"/etc/profiles/per-user/($env.USER)/bin/sweep"
    ]
    | where {|path| $path | path exists }
    | first
  )
  if ($args | is-not-empty) {
    ^$cli ...$args
    return
  }
  ^$cli status --json
  | from json
  | update size {|row| $row.size | if $in == null { null } else { into filesize } }
  | update due {|row| $row.due | if $in == null { null } else { into datetime } }
  | select state due size kind branch path reason
}
