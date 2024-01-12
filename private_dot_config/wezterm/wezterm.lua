local wezterm = require("wezterm")
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
config.hyperlink_rules = wezterm.default_hyperlink_rules()
config.scrollback_lines = 10000
config.quick_select_alphabet = "colemak"

return config
