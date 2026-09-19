return {
	"dariuscorvus/tree-sitter-surrealdb.nvim",
	dependencies = { "nvim-treesitter/nvim-treesitter" },
	event = { "BufRead *.surql" },
	config = function()
		require("tree-sitter-surrealdb").setup()
	end,
}
