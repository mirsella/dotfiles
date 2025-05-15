return {
	{
		"nvim-neo-tree/neo-tree.nvim",
		keys = {
			{ "<leader>e", false },
			{ "<leader>E", false },
		},
		init = function() end,
	},
	{
		"stevearc/oil.nvim",
		dependencies = { "echasnovski/mini.icons" },
		lazy = true,
		cmd = "Oil",
		event = { "VimEnter */*,.*", "BufNew */*,.*" },
		opts = {
			delete_to_trash = true,
			keymaps = {
				["<C-s>"] = false,
			},
		},
		keys = {
			{ "<leader>e", "<CMD>Oil<CR>", desc = "Open parent directory" },
			{ "-", "<CMD>Oil<CR>", desc = "Open parent directory" },
		},
	},
}
