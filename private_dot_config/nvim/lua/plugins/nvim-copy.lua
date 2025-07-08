return {
	"YounesElhjouji/nvim-copy",
	event = "VeryLazy",
	opts = {
		-- Configure files and directories to ignore using Gitignore-style patterns
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
	config = function(_, opts)
		require("nvim_copy").setup(opts)
	end,
	keys = {
		-- Group: Buffers (<leader>b)
		{
			"<leader>bc",
			function()
				vim.cmd("CopyBuffersToClipboard nofolds")
			end,
			desc = "Copy Visible Buffers (no folds)",
		},
		{
			"<leader>bC", -- Uppercase C for Current buffer
			function()
				vim.cmd("CopyCurrentBufferToClipboard nofolds")
			end,
			desc = "Copy Current Buffer (no folds)",
		},

		-- Group: Git (<leader>g)
		{
			"<leader>gc",
			function()
				vim.cmd("CopyGitFilesToClipboard nofolds")
			end,
			desc = "Copy Git Modified (no folds)",
		},

		-- Group: Quickfix/Compiler (<leader>c)
		{
			"<leader>cc", -- 'c' for copy, within the 'c' (compiler/code) group
			function()
				vim.cmd("CopyQuickfixFilesToClipboard nofolds")
			end,
			desc = "Copy Quickfix List (no folds)",
		},

		-- Group: Files (<leader>f)
		{
			"<leader>fd", -- 'f' for files, 'd' for directory
			function()
				-- Copies from current buffer's directory, recursively, no folds
				vim.cmd("CopyDirectoryFilesToClipboard nofolds")
			end,
			desc = "Copy Dir (recursive, no folds)",
		},
		{
			"<leader>fD", -- Uppercase D for non-recursive
			function()
				-- Copies from current buffer's directory, non-recursively, no folds
				vim.cmd("CopyDirectoryFilesToClipboard nofolds norecurse")
			end,
			desc = "Copy Dir (non-recursive, no folds)",
		},
	},
}
