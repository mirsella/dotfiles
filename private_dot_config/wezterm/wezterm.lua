local wezterm = require("wezterm")
local config = wezterm.config_builder()
local rose_pine = require("lua/rose-pine")

wezterm.on("update-status", function(window)
	local overrides = window:get_config_overrides() or {}
	local appearance = wezterm.gui.get_appearance()
	local scheme = appearance:find("Dark") and rose_pine.moon or rose_pine.dawn
	if overrides.colors ~= scheme then
		overrides.colors = scheme.colors()
		overrides.window_frame = scheme.window_frame()
		window:set_config_overrides(overrides)
	end
end)

-- config.enable_scroll_bar = true
config.font_size = 20.0
config.font = wezterm.font("JetBrains Mono")
config.window_padding = {
	left = 0,
	right = 0,
	top = 0,
	bottom = 0,
}
config.use_fancy_tab_bar = false
config.hide_tab_bar_if_only_one_tab = true
config.hyperlink_rules = wezterm.default_hyperlink_rules()
config.scrollback_lines = 100000
config.quick_select_alphabet = "colemak"
config.window_close_confirmation = "NeverPrompt"

-- config.window_decorations = "RESIZE"

wezterm.on("gui-startup", function(cmd)
	local _, _, window = wezterm.mux.spawn_window(cmd or {})
	window:gui_window():maximize()
end)

return config
