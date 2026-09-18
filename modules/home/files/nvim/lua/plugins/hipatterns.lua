return {
	{
		"nvim-mini/mini.hipatterns",
		opts = function(_, opts)
			require("config.hipatterns")(opts)
		end,
	},
}
