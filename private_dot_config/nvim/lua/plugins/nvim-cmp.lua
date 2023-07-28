return {
	"hrsh7th/nvim-cmp",
	dependencies = {
		{
			"petertriho/cmp-git",
			dependencies = { "nvim-lua/plenary.nvim" },
			config = function()
				require("cmp_git").setup()
			end,
		},
	},
	opts = function(_, opts)
		local cmp = require("cmp")
		opts.sources = cmp.config.sources(vim.list_extend(opts.sources, {
			{ name = "git" },
		}))
	end,
}
