return {
	"nvim-mini/mini.splitjoin",
	-- event = "VeryLazy",
	keys = { "<leader>sj", nil, desc = "Split and join lines" },
	opts = {
		mappings = {
			toggle = "<leader>sj",
			split = "",
			join = "",
		},
	},
}
