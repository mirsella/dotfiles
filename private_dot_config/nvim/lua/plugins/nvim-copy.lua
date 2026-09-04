return {
	"YounesElhjouji/nvim-copy",
	opts = {
		ignore = {
			"*node_modules/*",
			"*__pycache__/*",
			"*.git/*",
			"*dist/*",
			"*build/*",
			"*.log",
			"target/*",
		},
	},
	keys = {
		{
			"<leader>bc",
			"<cmd>CopyBuffersToClipboard nofolds<cr>",
			desc = "Copy Visible Buffers (no folds)",
		},
		{
			"<leader>bC",
			"<cmd>CopyCurrentBufferToClipboard nofolds<cr>",
			desc = "Copy Current Buffer (no folds)",
		},
		{
			"<leader>gc",
			"<cmd>CopyGitFilesToClipboard nofolds<cr>",
			desc = "Copy Git Modified (no folds)",
		},
		{
			"<leader>fd",
			"<cmd>CopyDirectoryFilesToClipboard nofolds<cr>",
			desc = "Copy Dir (recursive, no folds)",
		},
		{
			"<leader>fD",
			"<cmd>CopyDirectoryFilesToClipboard nofolds norecurse<cr>",
			desc = "Copy Dir (non-recursive, no folds)",
		},
	},
}
