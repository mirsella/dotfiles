local wezterm = require("wezterm")
local rose_pine = require("lua/rose-pine").main
local config = wezterm.config_builder()

-- config.colors = theme.colors()
-- config.window_frame = theme.window_frame()

function scheme_for_appearance(appearance)
	if appearance:find("Dark") then
		return "Builtin Solarized Dark"
	else
		return "Builtin Solarized Light"
	end
end

wezterm.on("window-config-reloaded", function(window, pane)
	local overrides = window:get_config_overrides() or {}
	local appearance = window:get_appearance()
	local colors = appearance:find("Dark") and rose_pine.moon or rose_pine.dawn
	if overrides.colors ~= colors then
		overrides.colors = colors
		window:set_config_overrides(overrides)
	end
end)

config.enable_scroll_bar = true
config.font_size = 20.0
config.window_padding = {
	left = 0,
	right = 0,
	top = 0,
	bottom = 0,
}
config.use_fancy_tab_bar = false
-- config.hide_tab_bar_if_only_one_tab = true

config.hyperlink_rules = wezterm.default_hyperlink_rules()
config.scrollback_lines = 10000
config.quick_select_alphabet = "colemak"
config.window_close_confirmation = "NeverPrompt"

-- start maximized
wezterm.on("gui-startup", function(cmd)
	local _, _, window = wezterm.mux.spawn_window(cmd or {})
	window:gui_window():maximize()
end)

return config
