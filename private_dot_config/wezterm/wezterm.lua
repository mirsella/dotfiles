local wezterm = require("wezterm")
local act = wezterm.action
local config = wezterm.config_builder()
local rose_pine = require("lua/rose-pine")

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
config.scrollback_lines = 100000
config.quick_select_alphabet = "colemak"
config.window_close_confirmation = "NeverPrompt"
config.animation_fps = 60

config.keys = {
	{ key = "f", mods = "CTRL|SHIFT", action = act.Search({ Regex = "" }) },
	{ key = "|", mods = "CTRL|SHIFT", action = act.SplitHorizontal },
	{ key = "-", mods = "CTRL|SHIFT", action = act.SplitVertical },

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
}

-- not working
-- wezterm.on("gui-startup", function(cmd)
-- 	local _, _, window = wezterm.mux.spawn_window(cmd or {})
-- 	window:gui_window():maximize()
-- end)

-- auto set theme variant
wezterm.on("update-status", function(window)
	local overrides = window:get_config_overrides() or {}
	local appearance = wezterm.gui.get_appearance()
	local scheme = appearance:find("Dark") and rose_pine.moon or rose_pine.dawn
	if overrides.colors ~= scheme then
		overrides.colors = scheme.colors()
		-- overrides.window_frame = scheme.window_frame()
		window:set_config_overrides(overrides)
	end
end)

return config
