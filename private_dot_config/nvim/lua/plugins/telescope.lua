return {
	"nvim-telescope/telescope.nvim",
	dependencies = {
		"nvim-lua/plenary.nvim",
		"debugloop/telescope-undo.nvim",
	},
	keys = {
		{ "<leader>st", false },
	},
	config = function()
		require("telescope").load_extension("undo")
		vim.keymap.set("n", "<leader>uu", "<cmd>Telescope undo<cr>", { desc = "Telescope undo" })
	end,
}
