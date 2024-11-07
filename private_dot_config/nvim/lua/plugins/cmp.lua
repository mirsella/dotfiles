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
	{
		"saghen/blink.cmp",
		optional = true,
		dependencies = {
			{ "saghen/blink.compat" },
			{ "dmitmel/cmp-digraphs", lazy = true },
			{ "hrsh7th/cmp-emoji", lazy = true },
			{ "chrisgrieser/cmp-nerdfont", lazy = true },
		},
		opts = {
			trigger = { completion = { keyword_range = "full" } },
			sources = {
				completion = {
					enabled_providers = { "digraphs", "emoji", "nerdfont" },
				},
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
