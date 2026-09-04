return {
	"nvim-treesitter/nvim-treesitter",
	event = function()
		return { "LazyFile" }
	end,
	opts = {
		auto_install = true,
	},
}
