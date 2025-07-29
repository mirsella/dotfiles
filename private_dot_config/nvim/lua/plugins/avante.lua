return {
	{
		"yetone/avante.nvim",
		event = "VeryLazy",
		dependencies = {
			"stevearc/dressing.nvim",
			"ibhagwan/fzf-lua",
		},
		opts = {
			-- Default configuration
			hints = { enabled = false },

			auto_suggestions_provider = nil, -- Since auto-suggestions are a high-frequency operation and therefore expensive, it is recommended to specify an inexpensive provider or even a free provider: copilot
			provider = "moonshot",
			providers = {
				moonshot = {
					__inherited_from = "openai",
					api_key_name = "GROQ_API_KEY",
					endpoint = "https://api.groq.com/openai/v1",
					model = "moonshotai/kimi-k2-instruct",
					extra_request_body = {
						temperature = 0.6,
					},
				},
			},

			-- File selector configuration
			--- @alias FileSelectorProvider "native" | "fzf" | "mini.pick" | "snacks" | "telescope" | string
			file_selector = {
				provider = "fzf", -- Avoid native provider issues
				provider_opts = {},
			},
		},
		build = LazyVim.is_win() and "powershell -ExecutionPolicy Bypass -File Build.ps1 -BuildFromSource false"
			or "make",
	},
	{
		"saghen/blink.cmp",
		lazy = true,
		dependencies = { "saghen/blink.compat" },
		opts = {
			sources = {
				default = { "avante_commands", "avante_mentions", "avante_files" },
				compat = {
					"avante_commands",
					"avante_mentions",
					"avante_files",
				},
				-- LSP score_offset is typically 60
				providers = {
					avante_commands = {
						name = "avante_commands",
						module = "blink.compat.source",
						score_offset = 90,
						opts = {},
					},
					avante_files = {
						name = "avante_files",
						module = "blink.compat.source",
						score_offset = 100,
						opts = {},
					},
					avante_mentions = {
						name = "avante_mentions",
						module = "blink.compat.source",
						score_offset = 1000,
						opts = {},
					},
				},
			},
		},
	},
	{
		"MeanderingProgrammer/render-markdown.nvim",
		optional = true,
		ft = function(_, ft)
			vim.list_extend(ft, { "Avante" })
		end,
		opts = function(_, opts)
			opts.file_types = vim.list_extend(opts.file_types or {}, { "Avante" })
		end,
	},
	{
		"folke/which-key.nvim",
		optional = true,
		opts = {
			spec = {
				{ "<leader>a", group = "ai" },
			},
		},
	},
	{
		"nvim-neo-tree/neo-tree.nvim",
		opts = function(_, opts)
			opts.filesystem = opts.filesystem or {}
			opts.filesystem.commands = opts.filesystem.commands or {}
			opts.filesystem.commands.avante_add_files = function(state)
				local node = state.tree:get_node()
				local filepath = node:get_id()
				local relative_path = require("avante.utils").relative_path(filepath)

				local sidebar = require("avante").get()

				local open = sidebar:is_open()
				-- ensure avante sidebar is open
				if not open then
					require("avante.api").ask()
					sidebar = require("avante").get()
				end

				sidebar.file_selector:add_selected_file(relative_path)

				-- remove neo tree buffer
				if not open then
					sidebar.file_selector:remove_selected_file("neo-tree filesystem [1]")
				end
			end
			opts.filesystem.window = opts.filesystem.window or {}
			opts.filesystem.window.mappings = opts.filesystem.window.mappings or {}
			opts.filesystem.window.mappings["oa"] = "avante_add_files"
		end,
	},
}
