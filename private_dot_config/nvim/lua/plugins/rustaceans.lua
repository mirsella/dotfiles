return {
	{
		"mrcjkb/rustaceanvim",
		opts = {
			server = {
				cmd = { vim.fn.expand("$HOME") .. "/.local/share/cargo/bin/lspmux" },
				default_settings = {
					["rust-analyzer"] = {
						procMacro = {
							enable = true,
							ignored = {
								["async-trait"] = { "async_trait" },
								["napi-derive"] = { "napi" },
								["async-recursion"] = { "async_recursion" },
								bevy_simple_subsecond_system_macros = { "hot" },
							},
						},
						diagnostics = {
							disabled = { "proc-macro-disabled" },
						},
						cargo = {
							targetDir = true,
							-- allFeatures = false,
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
	},
	{
		"neovim/nvim-lspconfig",
		opts = {
			servers = {
				["*"] = {
					keys = {
						-- codelens stuff
						{ "<leader>cc", false },
						{ "<leader>cC", false },
					},
				},
			},
		},
	},
}
