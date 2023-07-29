return {
	"gbprod/substitute.nvim",
	dependencies = { "gbprod/yanky.nvim" },
	keys = {
		{ "n", "gs", require("substitute").operator, { noremap = true, desc = "Substitute operator" } },
		{ "n", "gss", require("substitute").line, { noremap = true, desc = "Substitute line" } },
		{ "n", "gS", require("substitute").eol, { noremap = true, desc = "Substitute to end of line" } },
		{ "x", "gs", require("substitute").visual, { noremap = true, desc = "Substitute visual selection" } },
	},
	opts = {
		on_substitute = require("yanky.integration").substitute(),
	},
}
