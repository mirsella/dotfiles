return {
	{
		"nvim-neo-tree/neo-tree.nvim",
		enabled = false,
	},
	{
		"stevearc/oil.nvim",
		dependencies = { "echasnovski/mini.icons" },
		lazy = true,
		cmd = "Oil",
		event = { "VimEnter */*,.*", "BufNew */*,.*" },
		opts = {
			delete_to_trash = true,
		},
		keys = {
			{ "<leader>e", "<CMD>Oil<CR>", desc = "Open parent directory" },
			{ "-", "<CMD>Oil<CR>", desc = "Open parent directory" },
		},
	},
}
