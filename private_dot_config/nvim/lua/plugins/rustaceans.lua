return {
	"mrcjkb/rustaceanvim",
	opts = {
		server = {
			default_settings = {
				["rust-analyzer"] = {
					procMacro = {
						enable = true,
						ignored = {
							["async-trait"] = { "async_trait" },
							["napi-derive"] = { "napi" },
							["async-recursion"] = { "async_recursion" },
							["game_macros"] = { "complete_from_dir" },
						},
					},
					cargo = {
						targetDir = true,
					},
				},
			},
		},
	},
	keys = {
		{
			"<leader>cc",
			function()
				vim.cmd.RustLsp("openCargo")
			end,
			ft = "rust",
			desc = "Open Cargo.toml",
		},
		{
			"<leader>cC",
			function()
				vim.cmd.RustLsp("openDocs")
			end,
			mode = { "n", "v" },
			ft = { "rust", "markdown" },
			desc = "Open docs.rs document for symbol",
		},
	},
}
