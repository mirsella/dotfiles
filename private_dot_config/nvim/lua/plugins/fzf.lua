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
		opts = function(_, opts)
			local files = opts.actions and opts.actions.files or {}
			files["enter"] = require("fzf-lua").actions.file_edit
			files["alt-q"] = false
			files["alt-Q"] = false
			opts.actions = opts.actions or {}
			opts.actions.files = files
		end,
	},
	{
		"nvim-telescope/telescope.nvim",
		enabled = false,
	},
}
