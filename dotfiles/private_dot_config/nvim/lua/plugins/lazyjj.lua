return {
	"swaits/lazyjj.nvim",
	dependencies = "nvim-lua/plenary.nvim",
	opts = {},
	keys = {
		{
			"<leader>jj",
			function()
				require("lazyjj").open()
			end,
		},
	},
}
