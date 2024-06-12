return {
	"nvim-telescope/telescope.nvim",
	keys = {
		{
			"<leader>ff",
			function()
				require("telescope.builtin").find_files()
			end,
			desc = "Find Files (root dir)",
		},
		{
			"<leader>fF",
			function()
				require("telescope.builtin").find_files({ cwd = require("telescope.utils").buffer_dir() })
			end,
			desc = "Find Files (cwd)",
		},
		{ "<leader><space>", false },
		{
			"<leader>ft",
			function()
				require("telescope.builtin").filetype()
			end,
			{ silent = true, desc = "Change filetype with Telescope" },
		},
	},
}
