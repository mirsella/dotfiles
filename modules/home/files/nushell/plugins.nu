# plugins.nu - Minimal Nushell Plugin Manager

let $PLUGIN_DIR = $nu.data-dir | path join "plugins"

def ensure-plugin [repo: string] {
  if (which cargo | is-empty) {
    return
  }
  let name = ($repo | path basename | str replace ".git" "")
  let url = $"https://github.com/($repo)"
  let plugin_path = ($PLUGIN_DIR | path join $name)

  if not ($plugin_path | path exists) {
    print $"Installing ($name)..."
    try {
      git clone $url $plugin_path
      cargo build --release --manifest-path $"($plugin_path)/Cargo.toml" --locked
      plugin add $"($plugin_path)/target/release/($name)"
    } catch { |e|
      print $"Warning: ($name) install failed, continuing without it: ($e.msg)"
    }
  }
}

# Plugin sources track nushell main, but nixpkgs nu lags behind and an old
# runtime cannot talk to plugins built from HEAD (their protocol crates move
# in lockstep with nu). Only attempt installs on runtimes new enough for the
# current plugin set; re-evaluate the floor when it changes. A failed install
# must never break the rest of the shell, hence the try/catch above.
if (version | get version | split row "." | get 1 | into int) >= 115 {
  ensure-plugin "yybit/nu_plugin_compress"
  # ensure-plugin "fdncred/nu_plugin_file"
  ensure-plugin "FMotalleb/nu_plugin_clipboard"
  ensure-plugin "FMotalleb/nu_plugin_image"
  ensure-plugin "JosephTLyons/nu_plugin_units"
} else {
  print $"Skipping source-built plugins on nushell (version | get version) (needs >= 0.115)"
}
