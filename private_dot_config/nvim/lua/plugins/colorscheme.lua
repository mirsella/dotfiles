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
				-- Visual = { bg = "highlight_high" },
				Visual = { bg = "iris", blend = 15 },
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
