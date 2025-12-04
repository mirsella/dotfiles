return {
	"folke/sidekick.nvim",
	keys = {
		{
			"<c-e>",
			function()
				require("sidekick.").clear()
			end,
			desc = "Sidekick Toggle CLI",
		},
	},
}
