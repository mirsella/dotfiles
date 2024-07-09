return {
	{
		"rose-pine/neovim",
		name = "rose-pine",
		priority = 1000,
		opts = {
			dim_inactive_windows = true,
			extend_background_behind_borders = false,
			highlight_groups = {
				Visual = { bg = "highlight_high" },
				FzfLuaCursorLine = { bg = "#f2e9e1", fg = "text" },
			},
		},
	},
	{
		"LazyVim/LazyVim",
		opts = { colorscheme = function() end },
	},
	{ "folke/tokyonight.nvim", enabled = false },
	{ "catppuccin", enabled = false },
}
