return {
	"gbprod/substitute.nvim",
	dependencies = { "gbprod/yanky.nvim" },
	event = "VeryLazy",
	opts = {
		on_substitute = require("yanky.integration").substitute(),
	},
	config = function(_, opts)
		require("substitute").setup(opts)
		vim.keymap.set("n", "gs", require("substitute").operator, { noremap = true, desc = "Substitute operator" })
		vim.keymap.set("n", "gss", require("substitute").line, { noremap = true, desc = "Substitute line" })
		vim.keymap.set("n", "gS", require("substitute").eol, { noremap = true, desc = "Substitute to end of line" })
		vim.keymap.set("x", "gs", require("substitute").visual, { noremap = true, desc = "Substitute visual" })
	end,
}
