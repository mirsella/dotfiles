return {
	{
		"neovim/nvim-lspconfig",
		opts = {
			inlay_hints = {
				enabled = true,
			},
			servers = {
				["*"] = {
					keys = {
						-- codelens stuff
						{ "<leader>cc", false },
						{ "<leader>cC", false },
					},
				},
			},
		},
	},
}
