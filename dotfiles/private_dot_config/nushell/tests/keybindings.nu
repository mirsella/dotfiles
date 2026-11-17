# Run with: nu --no-config-file ~/.config/nushell/tests/keybindings.nu
use std/assert

let defaults = $env.config.keybindings
source ../config.nu

# Nushell 0.117 stores its menu bindings in config instead of adding them later.
for binding in $defaults {
  assert ($binding in $env.config.keybindings) $"Missing default keybinding: ($binding.name)"
}

for mode in [emacs vi_insert vi_normal] {
  let tab = ($env.config.keybindings
    | where modifier == none and keycode == tab
    | where {|binding| $mode in $binding.mode }
    | last)
  assert equal $tab.event.until.0 {send: menu, name: completion_menu}

  let complete = ($env.config.keybindings
    | where modifier == control and keycode == char_n
    | where {|binding| $mode in $binding.mode }
    | last)
  assert equal $complete.event.until.0 {send: menu, name: completion_menu}
}

print 'Default menu bindings and custom completion shortcuts are preserved.'
