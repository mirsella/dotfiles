return {
	"gbprod/cutlass.nvim",
	event = { "BufReadPost", "BufNewFile" },
	opts = {
		cut_key = "m",
		override_del = true,
		registers = {
			select = "s",
			delete = "d",
			change = "c",
		},
	},
}
