return {
	"folke/snacks.nvim",
	---@class snacks.explorer.Config
	opts = {
		explorer = {
			replace_netrw = false,
			enabled = false, -- using neotree extra
		},
	},
	keys = {
		{ "<leader>fe", false },
		{ "<leader>fE", false },
		{ "<leader>e", false },
		{ "<leader>E", false },
	},
}
