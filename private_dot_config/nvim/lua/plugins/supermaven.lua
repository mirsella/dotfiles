return {
	{
		"supermaven-inc/supermaven-nvim",
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
				group_index = 1,
				priority = 100,
			})
		end,
	},
	{
		"nvim-lualine/lualine.nvim",
		optional = true,
		event = "VeryLazy",
		opts = function(_, opts)
			table.insert(opts.sections.lualine_x, 2, LazyVim.lualine.cmp_source("supermaven"))
		end,
	},
	{
		"folke/noice.nvim",
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
