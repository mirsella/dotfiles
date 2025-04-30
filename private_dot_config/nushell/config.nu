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
$env.config.float_precision = 4
$env.config.keybindings = [
  {
    name: menu_accept
    modifier: control
    keycode: char_y
    mode: [emacs, vi_normal, vi_insert]
    event: { send: Enter }
  }
  {
    name: completion_menu_open
    modifier: control
    keycode: char_n
    mode: [emacs, vi_normal, vi_insert]
    event: {
      until: [
        { send: menu name: completion_menu }
        { send: menunext }
        { edit: complete }
      ]
    }
  }
  {
    name: accept_history_suggestion
    modifier: control
    keycode: char_y
    mode: [emacs, vi_insert]
    event: { send: HistoryHintComplete }
  }
]

if ("~/.config/token/hathora.token" | path exists) {
  open ~/.config/token/hathora.token | parse "{key}={value}" | transpose -r -d | load-env
}

let autoload_path = ($nu.data-dir | path join "vendor/autoload")
mkdir $autoload_path
starship init nu | save -f ($autoload_path | path join "starship.nu")
zoxide init --cmd cd nushell | save -f ($autoload_path | path join "zoxide.nu")
jj util completion nushell | save -f ($autoload_path | path join "jj.nu")
