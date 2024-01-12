local wezterm = require("wezterm")
local act = wezterm.action
local config = wezterm.config_builder()

config.colors = require("rose-pine/lua/rose-pine-moon").colors()
config.window_frame = require("rose-pine/lua/rose-pine-moon").window_frame()
config.colors.scrollbar_thumb = "#5c6370"
config.enable_scroll_bar = true
config.font_size = 20.0
config.window_padding = {
	left = 0,
	right = 0,
	top = 0,
	bottom = 0,
}
config.use_fancy_tab_bar = false
config.hide_tab_bar_if_only_one_tab = true

config.hyperlink_rules = wezterm.default_hyperlink_rules()
config.scrollback_lines = 10000
config.quick_select_alphabet = "colemak"

-- start maximized
wezterm.on("gui-startup", function(cmd)
	local _, _, window = wezterm.mux.spawn_window(cmd or {})
	window:gui_window():maximize()
end)

return config
