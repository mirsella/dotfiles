return {
	{
		"rose-pine/neovim",
		name = "rose-pine",
		opts = {
			dim_inactive_windows = true,
			highlight_groups = {
				Visual = { bg = "highlight_high" },
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
