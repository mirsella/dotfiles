local wezterm = require("wezterm")
local act = wezterm.action
local config = wezterm.config_builder()
local rose_pine = require("lua/rose-pine")

-- config.enable_scroll_bar = true
config.font_size = 14.0
config.font = wezterm.font("JetBrains Mono")
config.window_padding = {
	left = 0,
	right = 0,
	top = 0,
	bottom = 0,
}
config.use_fancy_tab_bar = false
config.hide_tab_bar_if_only_one_tab = true
config.scrollback_lines = 1000000
config.quick_select_alphabet = "colemak"
config.window_close_confirmation = "NeverPrompt"
config.animation_fps = 1 -- less resource usage
config.cursor_blink_ease_in = "Constant"
config.cursor_blink_ease_out = "Constant"
config.enable_wayland = false -- wayland screen resolution broken https://github.com/wez/wezterm/issues/6262
config.warn_about_missing_glyphs = false

config.keys = {
	{ key = "f", mods = "CTRL|SHIFT", action = act.Search({ Regex = "" }) },
	{ key = "|", mods = "CTRL|SHIFT", action = act.SplitHorizontal },

	{ key = "F1", mods = "NONE", action = "ActivateCopyMode" },
	{ key = "F2", mods = "NONE", action = "QuickSelect" },
	{
		key = "F3",
		mods = "NONE",
		action = act.QuickSelectArgs({
			label = "open url",
			patterns = {
				"\\((https?://\\S+)\\)",
				"\\[(https?://\\S+)\\]",
				"\\{(https?://\\S+)\\}",
				"<(https?://\\S+)>",
				"\\bhttps?://\\S+[)/a-zA-Z0-9-]+",
			},
			action = wezterm.action_callback(function(window, pane)
				local url = window:get_selection_text_for_pane(pane)
				wezterm.log_info("opening: " .. url)
				wezterm.open_with(url)
			end),
		}),
	},
	{
		key = "Enter",
		mods = "ALT",
		action = wezterm.action.DisableDefaultAssignment,
	},

	{
		key = "n",
		mods = "SUPER",
		action = wezterm.action_callback(function(win, pane)
			local tab, window = pane:move_to_new_window()
		end),
	},
}

-- auto set theme variant
wezterm.on("update-status", function(window)
	local overrides = window:get_config_overrides() or {}
	local appearance = window:get_appearance()
	local scheme = appearance:find("Dark") and rose_pine.moon or rose_pine.dawn
	if overrides.colors ~= scheme then
		overrides.colors = scheme.colors()
		overrides.window_frame = scheme.window_frame()
		window:set_config_overrides(overrides)
	end
end)

return config
