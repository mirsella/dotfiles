return {
	"folke/snacks.nvim",
	---@class snacks.explorer.Config
	opts = {
		explorer = {
			replace_netrw = false,
		},
	},
	keys = {
		{
			"<leader>fe",
			function()
				Snacks.explorer({ cwd = LazyVim.root() })
			end,
			desc = "Explorer Snacks (root dir)",
		},
		{
			"<leader>fE",
			function()
				Snacks.explorer()
			end,
			desc = "Explorer Snacks (cwd)",
		},
		{ "<leader>e", false },
		{ "<leader>E", false },
	},
}
