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
		"nvim-cmp",
		opts = function(_, opts)
			table.insert(opts.sources, 1, {
				name = "supermaven",
				priority = 100,
			})
		end,
	},
}
