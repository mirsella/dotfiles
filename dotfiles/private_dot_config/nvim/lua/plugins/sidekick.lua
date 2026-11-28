return {
	"folke/sidekick.nvim",
	opts = {
		nes = { enabled = false },
		copilot = { status = { enabled = false } },
	},
	keys = {
		{ "<tab>", false, mode = "n" },
	},
	config = function(_, opts)
		require("sidekick").setup(opts)
		-- Sidekick merges its built-in CLI tools during setup.
		require("sidekick.config").cli.tools.copilot = nil

		-- The LazyVim extra still registers NES shortcuts when NES is disabled.
		LazyVim.cmp.actions.ai_nes = nil
		if vim.fn.maparg("<leader>uN", "n", false, true).desc == "Toggle Sidekick NES" then
			vim.keymap.del("n", "<leader>uN")
		end
	end,
}
