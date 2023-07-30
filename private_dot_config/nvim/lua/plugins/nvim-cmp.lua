return {
	"hrsh7th/nvim-cmp",
	event = "InsertEnter",
	lazy = true,
	dependencies = {
		{
			"petertriho/cmp-git",
			lazy = true,
			-- config = function()
			-- 	require("cmp_git").setup()
			-- end,
		},
		{
			"jcdickinson/codeium.nvim",
			commit = "b1ff0d6c993e3d87a4362d2ccd6c660f7444599f", -- see https://github.com/jcdickinson/codeium.nvim/pull/74
			lazy = true,
			-- config = function()
			-- 	require("codeium").setup()
			-- end,
		},
		"nvim-lua/plenary.nvim",
	},
	opts = function(_, opts)
		local cmp = require("cmp")
		opts.sources = cmp.config.sources(vim.list_extend(opts.sources, {
			{ name = "git" },
			{ name = "copilot" },
			{ name = "codeium" },
		}))
	end,
}
