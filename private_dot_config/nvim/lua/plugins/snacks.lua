return {
	"folke/snacks.nvim",
	---@class snacks.explorer.Config
	opts = {
		explorer = {
			replace_netrw = false,
		},
	},
	keys = {
		{ "<leader>fe", false },
		{ "<leader>fE", false },
		{ "<leader>e", false },
		{ "<leader>E", false },
	},
}
