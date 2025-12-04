return {
	{
		"hrsh7th/nvim-cmp",
		optional = true,
		dependencies = { "dmitmel/cmp-digraphs", "hrsh7th/cmp-emoji", "chrisgrieser/cmp-nerdfont" },
		opts = function(_, opts)
			-- table.insert(opts.sources, { name = "digraphs", score_offset = -3 })
			table.insert(opts.sources, { name = "emoji" })
			table.insert(opts.sources, { name = "nerdfont" })
		end,
	},
	-- {
	-- 	"zbirenbaum/copilot.lua",
	-- 	dependencies = {
	-- 		"copilotlsp-nvim/copilot-lsp", -- (optional) for NES functionality
	-- 	},
	-- },
	-- {
	-- 	"copilotlsp-nvim/copilot-lsp",
	-- 	init = function()
	-- 		vim.g.copilot_nes_debounce = 500
	-- 		vim.lsp.enable("copilot_ls")
	-- 		vim.keymap.set("n", "<tab>", function()
	-- 			local bufnr = vim.api.nvim_get_current_buf()
	-- 			local state = vim.b[bufnr].nes_state
	-- 			if state then
	-- 				-- Try to jump to the start of the suggestion edit.
	-- 				-- If already at the start, then apply the pending suggestion and jump to the end of the edit.
	-- 				local _ = require("copilot-lsp.nes").walk_cursor_start_edit()
	-- 					or (
	-- 						require("copilot-lsp.nes").apply_pending_nes()
	-- 						and require("copilot-lsp.nes").walk_cursor_end_edit()
	-- 					)
	-- 				return nil
	-- 			else
	-- 				-- Resolving the terminal's inability to distinguish between `TAB` and `<C-i>` in normal mode
	-- 				return "<C-i>"
	-- 			end
	-- 		end, { desc = "Accept Copilot NES suggestion", expr = true })
	-- 	end,
	-- },
	{
		"saghen/blink.cmp",
		optional = true,
		dependencies = {
			{ "saghen/blink.compat" },
			{ "dmitmel/cmp-digraphs" },
			{ "hrsh7th/cmp-emoji" },
			{ "chrisgrieser/cmp-nerdfont" },
		},
		opts = {
			keymap = {
				["<Tab>"] = {
					"snippet_forward",
					function() -- sidekick next edit suggestion
						return require("sidekick").nes_jump_or_apply()
					end,
					function() -- if you are using Neovim's native inline completions
						return vim.lsp.inline_completion.get()
					end,
					"fallback",
				},
			},
			-- trigger = { completion = { keyword_range = "full" } },
			fuzzy = { prebuilt_binaries = { download = true } },
			sources = {
				default = { "digraphs", "emoji", "nerdfont" },
				providers = {
					digraphs = {
						name = "digraphs",
						module = "blink.compat.source",
						override = {
							get_trigger_characters = function()
								return { "." }
							end,
						},
						transform_items = function(ctx, items)
							local kind = require("blink.cmp.types").CompletionItemKind.Text
							for i, _ in ipairs(items) do
								items[i].kind = kind
							end
							return items
						end,
						score_offset = -10,
					},
					nerdfont = {
						name = "nerdfont",
						module = "blink.compat.source",
						score_offset = -5,
						transform_items = function(ctx, items)
							local kind = require("blink.cmp.types").CompletionItemKind.Text
							for i, _ in ipairs(items) do
								items[i].kind = kind
							end
							return items
						end,
					},
					emoji = {
						name = "emoji",
						module = "blink.compat.source",
						score_offset = -5,
						transform_items = function(ctx, items)
							local kind = require("blink.cmp.types").CompletionItemKind.Text
							for i, _ in ipairs(items) do
								items[i].kind = kind
							end
							return items
						end,
					},
				},
			},
		},
	},
}
