return {
	{
		"folke/flash.nvim",
		disable = true,
	},
	{
		"ggandor/flit.nvim",
		disable = true,
	},
	{
		"ggandor/leap.nvim",
		disable = true,
	},
	{
		"phaazon/hop.nvim",
		config = function()
			require("hop").setup()
			vim.keymap.set("n", "s", "<cmd>HopWord<cr>")
			-- vim.keymap.set("n", "s", "<cmd>HopWord<cr>")
		end,
	},
}
