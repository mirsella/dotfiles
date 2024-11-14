return {
	{
		"supermaven-inc/supermaven-nvim",
		event = { "BufReadPre", "BufNewFile" },
		build = ":SupermavenUseFree",
		opts = {
			disable_inline_completion = true, -- disables inline completion for use with cmp
			disable_keymaps = true, -- disables built in keymaps for more manual control
		},
	},
	{
		"hrsh7th/nvim-cmp",
		optional = true,
		opts = function(_, opts)
			table.insert(opts.sources, 1, {
				name = "supermaven",
				-- group_index = 1,
				priority = 100,
			})
		end,
	},
	{
		"saghen/blink.cmp",
		optional = true,
		dependencies = { "saghen/blink.compat" },
		sources = {
			completion = {
				enabled_providers = { "supermaven" },
			},
			providers = {
				supermaven = {
					name = "supermaven",
					module = "blink.compat.source",
					priority = 100,
				},
			},
		},
	},
	{
		"folke/noice.nvim",
		optional = true,
		opts = function(_, opts)
			vim.list_extend(opts.routes, {
				{
					filter = {
						event = "msg_show",
						any = {
							{ find = "Starting Supermaven" },
							{ find = "Supermaven Free Tier" },
						},
					},
					skip = true,
				},
			})
		end,
	},
}
