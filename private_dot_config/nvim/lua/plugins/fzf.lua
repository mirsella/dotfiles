return {
	{
		"ibhagwan/fzf-lua",
		keys = {
			{
				"<leader>ff",
				function()
					require("fzf-lua").files()
				end,
				desc = "Find Files (Root Dir)",
			},
			{
				"<leader>fF",
				function()
					require("fzf-lua").files({ cwd = vim.fn.expand("%:p:h") })
				end,
				desc = "Find Files (Buffer Dir)",
			},
			{ "<leader><leader>", enabled = false },
			{
				"<leader>ft",
				function()
					require("fzf-lua").filetypes()
				end,
				desc = "Change filetype",
			},
		},
	},
	{
		"nvim-telescope/telescope.nvim",
		enabled = false,
	},
}
