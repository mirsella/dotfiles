local variants = { dark = "rose-pine-moon", light = "rose-pine-dawn" }

local function set_variant(background)
	if vim.g.colors_name ~= "rose-pine" or vim.o.background ~= background then
		vim.cmd.colorscheme(variants[background])
	end
end

return {
	{
		"rose-pine/neovim",
		name = "rose-pine",
		priority = 10000,
		opts = {
			variant = "auto",
			dim_inactive_windows = true,
			extend_background_behind_borders = false,
			highlight_groups = {
				Visual = { bg = "iris", blend = 15 },
			},
		},
	},
	{
		"LazyVim/LazyVim",
		opts = {
			colorscheme = function()
				set_variant(vim.o.background)
			end,
		},
	},
	{
		"f-person/auto-dark-mode.nvim",
		event = { "CursorHold", "CursorHoldI" },
		init = function()
			local result = vim.system({
				"dbus-send",
				"--session",
				"--print-reply=literal",
				"--reply-timeout=1000",
				"--dest=org.freedesktop.portal.Desktop",
				"/org/freedesktop/portal/desktop",
				"org.freedesktop.portal.Settings.Read",
				"string:org.freedesktop.appearance",
				"string:color-scheme",
			}, { text = true }):wait(1000)
			local preference = result.code == 0 and (result.stdout or ""):match("uint32%s+([012])")
			if not preference then
				vim.schedule(function()
					vim.notify("Could not detect the system color scheme", vim.log.levels.WARN)
				end)
				return
			end
			vim.o.background = preference == "1" and "dark" or "light"
		end,
		opts = {
			update_interval = 1000,
			set_dark_mode = function()
				set_variant("dark")
			end,
			set_light_mode = function()
				set_variant("light")
			end,
		},
	},
	{ "folke/tokyonight.nvim", enabled = false },
	{ "catppuccin", enabled = false },
}
