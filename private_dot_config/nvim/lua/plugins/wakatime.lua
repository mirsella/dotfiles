return {
	"wakatime/vim-wakatime",
	event = { "CursorHold", "CursorHoldI" },
	init = function()
		vim.g.loaded_wakatime = 1
	end,
	opts = { status_bar_enabled = false },
	config = function(_, opts)
		require("wakatime").setup(opts)
	end,
}
