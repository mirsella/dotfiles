# plugins.nu - Minimal Nushell Plugin Manager

let $PLUGIN_DIR = $nu.data-dir | path join "plugins"

def ensure-plugin [repo: string] {
  let name = ($repo | path basename | str replace ".git" "")
  let url = $"https://github.com/($repo)"
  let plugin_path = ($PLUGIN_DIR | path join $name)
  
  if not ($plugin_path | path exists) {
    print $"Installing ($name)..."
    git clone $url $plugin_path
    cargo build --release --manifest-path $"($plugin_path)/Cargo.toml" --locked
    plugin add $"($plugin_path)/target/release/($name)"
  }
}

ensure-plugin "yybit/nu_plugin_compress"
# ensure-plugin "fdncred/nu_plugin_file"
ensure-plugin "FMotalleb/nu_plugin_clipboard"
ensure-plugin "FMotalleb/nu_plugin_image"
ensure-plugin "JosephTLyons/nu_plugin_units"
