return {
	"Saecki/crates.nvim",
	keys = {
		{
			"<leader>cf",
			require("crates").show_features_popup,
			{ silent = true, desc = "Show features popup in Cargo.toml" },
		},
	},
}
