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
		dependencies = {
			"nvim-mini/mini.icons",
			"refractalize/oil-git-status.nvim",
		},
		lazy = vim.fn.argc(-1) == 0 or vim.fn.isdirectory(vim.fn.argv(0)) == 0,
		cmd = "Oil",
		opts = {
			delete_to_trash = true,
			keymaps = {
				["<C-s>"] = false,
			},
			win_options = {
				signcolumn = "yes:2",
			},
		},
		config = function(_, opts)
			require("oil").setup(opts)
			require("oil-git-status").setup()
		end,
		keys = {
			{ "<leader>e", "<CMD>Oil<CR>", desc = "Open parent directory" },
			{ "-", "<CMD>Oil<CR>", desc = "Open parent directory" },
		},
	},
}
