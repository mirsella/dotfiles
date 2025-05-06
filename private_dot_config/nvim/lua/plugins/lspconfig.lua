return {
	{
		"neovim/nvim-lspconfig",
		opts = {
			inlay_hints = {
				enabled = true,
			},
		},
		init = function()
			local keys = require("lazyvim.plugins.lsp.keymaps").get()
			-- disable a keymap
			keys[#keys + 1] = { "<leader>cc", false }
			keys[#keys + 1] = { "<leader>cC", false }
		end,
	},
}
