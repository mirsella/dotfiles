return {
	"dariuscorvus/tree-sitter-surrealdb.nvim",
	dependencies = { "nvim-treesitter/nvim-treesitter" },
	event = "VeryLazy",
	config = function()
		require("tree-sitter-surrealdb").setup()
	end,
}
