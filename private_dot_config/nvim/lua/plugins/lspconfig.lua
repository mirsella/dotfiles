return {
	{
		"neovim/nvim-lspconfig",
		opts = {
			inlay_hints = {
				enabled = true,
			},
		},
		keys = {
			{ "<leader>cc", false },
			{ "<leader>cC", false },
		},
	},
}
