return {
	"Saecki/crates.nvim",
	keys = {
		{
			"<leader>cf",
			function()
				require("crates").show_features_popup()
			end,
			ft = "toml",
			desc = "Show features popup in Cargo.toml",
		},
	},
}
