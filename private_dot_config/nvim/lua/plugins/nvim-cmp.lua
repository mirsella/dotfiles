return {
	"hrsh7th/nvim-cmp",
	dependencies = {
		{
			"petertriho/cmp-git",
			config = function()
				require("cmp_git").setup()
			end,
		},
		{
			"jcdickinson/codeium.nvim",
			commit = "b1ff0d6c993e3d87a4362d2ccd6c660f7444599f", -- see https://github.com/jcdickinson/codeium.nvim/pull/74
			lazy = false,
			config = function()
				require("codeium").setup()
			end,
		},
		"nvim-lua/plenary.nvim",
	},
	opts = function(_, opts)
		local cmp = require("cmp")
		opts.sources = cmp.config.sources(vim.list_extend(opts.sources, {
			{ name = "git" },
			{ name = "codeium" },
			{ name = "copilot" },
		}))
	end,
}
