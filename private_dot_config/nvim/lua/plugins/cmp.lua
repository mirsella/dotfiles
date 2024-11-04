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
			sources = {
				completion = {
					enabled_providers = { "digraphs", "emoji", "nerdfont" },
				},
				providers = {
					digraphs = {
						name = "digraphs",
						module = "blink.compat.source",
						enabled = false, -- NOTE: trigger even without the . prefix
						score_offset = -3,
					},
					emoji = {
						name = "emoji",
						module = "blink.compat.source",
					},
					nerdfont = {
						name = "nerdfont",
						module = "blink.compat.source",
					},
				},
			},
		},
	},
}
