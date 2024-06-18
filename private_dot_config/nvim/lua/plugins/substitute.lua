return {
	"gbprod/substitute.nvim",
	dependencies = { "gbprod/yanky.nvim" },
	keys = {
		{
			"gs",
			function()
				require("substitute").operator()
			end,
      mode = { "n", "v" },
			desc = "Substitute operator",
		},
		{
			"gss",
			function()
				require("substitute").line()
			end,
			desc = "Substitute line",
		},
		{
			"gS",
			function()
				require("substitute").eol()
			end,
			desc = "Substitute to end of line",
		},
	},
	opts = {
		on_substitute = require("yanky.integration").substitute(),
	},
}
