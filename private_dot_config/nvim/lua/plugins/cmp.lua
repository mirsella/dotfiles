return {
	{
		"hrsh7th/nvim-cmp",
		optional = true,
		dependencies = { "dmitmel/cmp-digraphs", "hrsh7th/cmp-emoji", "chrisgrieser/cmp-nerdfont" },
		opts = function(_, opts)
			table.insert(opts.sources, { name = "digraphs" })
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
			signature_help = {
				enabled = true,
			},
			sources = {
				completion = {
					-- TODO: not working :(
					enabled_providers = { "digraphs", "emoji", "nerdfont" },
				},
				providers = {
					digraphs = {
						name = "digraphs",
						module = "blink.compat.source",
						-- score_offset = -3,
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
