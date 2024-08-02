return {
	"Saecki/crates.nvim",
	lazy = true,
	keys = {
		{
			"<leader>cf",
			require("crates").show_features_popup,
			silent = true,
			ft = "toml",
			desc = "Show features popup in Cargo.toml",
		},
	},
}
