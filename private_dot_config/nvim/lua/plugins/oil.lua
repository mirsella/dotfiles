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
		dependencies = { "nvim-mini/mini.icons" },
		lazy = true,
		cmd = "Oil",
		event = { "VimEnter */*,.*", "BufNew */*,.*" },
		opts = {
			delete_to_trash = true,
			keymaps = {
				["<C-s>"] = false,
			},
			win_options = {
				signcolumn = "yes:2",
			},
		},
		keys = {
			{ "<leader>e", "<CMD>Oil<CR>", desc = "Open parent directory" },
			{ "-", "<CMD>Oil<CR>", desc = "Open parent directory" },
		},
	},
	{
		"refractalize/oil-git-status.nvim",
		config = true,
	},
}
