return {
	"gbprod/substitute.nvim",
	dependencies = { "gbprod/yanky.nvim" },
	keys = {
		{
			"gs",
			function()
				require("substitute").operator()
			end,
			noremap = true,
			desc = "Substitute operator",
		},
		{
			"gss",
			function()
				require("substitute").line()
			end,
			noremap = true,
			desc = "Substitute line",
		},
		{
			"gS",
			function()
				require("substitute").eol()
			end,
			noremap = true,
			desc = "Substitute to end of line",
		},
		{
			"gs",
			function()
				require("substitute").visual()
			end,
			mode = { "x" },
			noremap = true,
			desc = "Substitute visual",
		},
	},
	opts = {
		on_substitute = require("yanky.integration").substitute(),
	},
	-- config = function(_, opts)
	-- 	require("substitute").setup(opts)
	-- 	vim.keymap.set("n", "gs", require("substitute").operator, { noremap = true, desc = "Substitute operator" })
	-- 	vim.keymap.set("n", "gss", require("substitute").line, { noremap = true, desc = "Substitute line" })
	-- 	vim.keymap.set("n", "gS", require("substitute").eol, { noremap = true, desc = "Substitute to end of line" })
	-- 	vim.keymap.set("x", "gs", require("substitute").visual, { noremap = true, desc = "Substitute visual" })
	-- end,
}
