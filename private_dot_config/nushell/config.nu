source completions.nu
source functions.nu
source alias.nu
source plugins.nu

$env.config.buffer_editor = "nvim"
$env.config.show_banner = false
# $env.config.edit_mode = 'vi'
$env.config.history.max_size = 100000
$env.config.rm.always_trash = true
$env.config.highlight_resolved_externals = true

# if ("~/.gtkrc-2.0" | path exists) {
#   rm -v ~/.gtkrc-2.0 &
# }

do --ignore-errors {
  open ~/.config/token/hathora.token | parse "{key}={value}" | transpose -r -d | load-env
}

let autoload_path = ($nu.data-dir | path join "vendor/autoload")
mkdir $autoload_path
starship init nu | save -f ($autoload_path | path join "starship.nu")
zoxide init --cmd cd nushell | save -f ($autoload_path | path join "zoxide.nu")
jj util completion nushell | save -f ($autoload_path | path join "jj.nu")
