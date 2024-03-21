return {
	"f-person/auto-dark-mode.nvim",
	lazy = false,
	priority = 10000,
	opts = {
		update_interval = 1000,
		set_dark_mode = function()
			vim.cmd("colorscheme rose-pine-moon")
		end,
		set_light_mode = function()
			vim.cmd("colorscheme rose-pine")
		end,
	},
}
