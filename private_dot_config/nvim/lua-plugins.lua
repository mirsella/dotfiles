require('impatient')
require('hop').setup()
require('nvim-web-devicons').setup()
require('lualine').setup()
require('colorizer').setup()
require('telescope').setup()
require('cloak').setup()

require("trouble").setup{
	width = 0,
}

require("copilot").setup({
	suggestion = {
		enabled = false,
		auto_trigger = true
	},
	panel = {
		enabled = false,
		auto_refresh = true
	},
})

require('nvim-treesitter.configs').setup {
	context_commentstring = {
		enable = true
	},
	-- ensure_installed = { "c", "cpp", "lua", "vim", "vue", "typescript", "javascript", "json", "html", "css", "bash", "python", "rust", "java", "regex" },
	-- Automatically install missing parsers when entering buffer
	-- Recommendation: set to false if you don't have `tree-sitter` CLI installed locally
	auto_install = true,
	highlight = {
		enable = true,
		additional_vim_regex_highlighting = false,
	},
}

require('bufferline').setup {
	options = {
		hover = {
			enabled = true,
			delay = 0,
			reveal = {'close'}
		},
		numbers = 'buffer_id',
		indicator = {
			style = 'none'
		},
		diagnostics = 'nvim_lsp',
		diagnostics_indicator = function(count, level, diagnostics_dict, context)
			local s = ''
			for e, n in pairs(diagnostics_dict) do
				local sym = e == 'error' and ' '
				or (e == 'warning' and ' ' or ' ')
				s = s .. n .. sym
			end
			return s
		end,
		tab_size = 0,
		diagnostics_update_in_insert = true,
		show_tab_indicators = false,
	}
}

-- Set up nvim-cmp.
require("cmp_git").setup()
require("copilot_cmp").setup {
	formatters = {
		insert_text = require("copilot_cmp.format").remove_existing
	},
}
local cmp = require('cmp')
local lspkind = require('lspkind')
cmp.setup({
	snippet = {
		-- REQUIRED - you must specify a snippet engine
		expand = function(args)
			vim.fn["vsnip#anonymous"](args.body)
		end,
	},
	window = {
		completion = cmp.config.window.bordered(),
		documentation = cmp.config.window.bordered(),
	},
	mapping = cmp.mapping.preset.insert({
		['<C-u>'] = cmp.mapping.scroll_docs(-4),
		['<C-d>'] = cmp.mapping.scroll_docs(4),
		['<C-p>'] = cmp.mapping.select_prev_item({behavior = cmp.SelectBehavior.Select}),
		['<C-n>'] = cmp.mapping.select_next_item({behavior = cmp.SelectBehavior.Select}),
		['<C-e>'] = cmp.mapping.abort(),
		['<CR>'] = cmp.mapping.confirm({select = true,	behavior = cmp.ConfirmBehavior.Replace}),
		["<C-Space>"] = cmp.mapping.complete(),

	}),
	sources = cmp.config.sources({
		{ name = 'path', priority = 5 },
		{ name = 'git', priority = 4 },
		{ name = 'copilot', priority = 3 },
		{ name = 'nvim_lsp_signature_help', priority = 2 },
		{ name = 'nvim_lua', priority = 1 },
		{ name = 'nvim_lsp', priority = 1 },
		{ name = 'treesitter', priority = 2 },
		{ name = 'vsnip', priority = 1 },
		-- { name = 'buffer', priority = 1, keyword_length = 3 },
	}, {
		{ name = 'buffer', priority = 1, keyword_length = 3 },
	}),
	formatting = {
		format = lspkind.cmp_format({
			mode = 'symbol', -- show only symbol annotations
			maxwidth = 50, -- prevent the popup from showing more than provided characters (e.g 50 will not show more than 50 characters)
			ellipsis_char = '...', -- when popup menu exceed maxwidth, the truncated part would show ellipsis_char instead (must define maxwidth first)
			symbol_map = { Copilot = "" }
		})
	},
	-- experimental = { ghost_text = true },
})

-- Use buffer source for `/` and `?` (if you enabled `native_menu`, this won't work anymore).
cmp.setup.cmdline({ '/', '?' }, {
	mapping = cmp.mapping.preset.cmdline(),
	sources = {
		{ name = 'buffer' }
	}

})

-- Use cmdline & path source for ':' (if you enabled `native_menu`, this won't work anymore).
cmp.setup.cmdline(':', {
	mapping = cmp.mapping.preset.cmdline(),
	sources = cmp.config.sources({
		{ name = 'path' }
	}, {
		{ name = 'cmdline' }
	})
})

-- If you want insert `(` after select function or method item
require('nvim-autopairs').setup()
local cmp_autopairs = require('nvim-autopairs.completion.cmp')
cmp.event:on(
'confirm_done',
cmp_autopairs.on_confirm_done()
)

-- Set up lspconfig.
local capabilities = require('cmp_nvim_lsp').default_capabilities()

require('mason').setup()
require('mason-lspconfig').setup {
	ensure_installed = { 'pyright', 'lua_ls', 'tsserver', 'bashls', 'jdtls', 'tailwindcss', 'vimls', 'volar', 'clangd', 'rust_analyzer', 'emmet_ls'},
	automatic_installation = true,
}
require("mason-lspconfig").setup_handlers {
	-- The first entry (without a key) will be the default handler
	-- and will be called for each installed server that doesn't have
	-- a dedicated handler.
	function (server_name) -- default handler (optional)
		require("lspconfig")[server_name].setup {
			capabilities = capabilities,
		}
	end,
	-- Next, you can provide a dedicated handler for specific servers.
	-- For example, a handler override for the `rust_analyzer`:
	["rust_analyzer"] = function ()
		require("rust-tools").setup {}
	end
}

-- vim.api.nvim_create_augroup("bufcheck", { clear = true})
-- vim.api.nvim_create_autocmd("FileType", {
-- 	group = "bufcheck",
-- 	pattern = "Trouble",
-- 	command = "set wrap"
-- })

vim.keymap.set("n", "gd", function() vim.lsp.buf.definition() end)
vim.keymap.set("n", "<leader>ca", function() vim.lsp.buf.code_action() end)

require("neo-tree").setup({
	default_component_configs = {
		icon = {
			folder_empty = "󰜌",
			folder_empty_open = "󰜌",
		},
		git_status = {
			symbols = {
				renamed   = "󰁕",
				unstaged  = "󰄱",
			},
		},
	},
	document_symbols = {
		kinds = {
			File = { icon = "󰈙", hl = "Tag" },
			Namespace = { icon = "󰌗", hl = "Include" },
			Package = { icon = "󰏖", hl = "Label" },
			Class = { icon = "󰌗", hl = "Include" },
			Property = { icon = "󰆧", hl = "@property" },
			Enum = { icon = "󰒻", hl = "@number" },
			Function = { icon = "󰊕", hl = "Function" },
			String = { icon = "󰀬", hl = "String" },
			Number = { icon = "󰎠", hl = "Number" },
			Array = { icon = "󰅪", hl = "Type" },
			Object = { icon = "󰅩", hl = "Type" },
			Key = { icon = "󰌋", hl = "" },
			Struct = { icon = "󰌗", hl = "Type" },
			Operator = { icon = "󰆕", hl = "Operator" },
			TypeParameter = { icon = "󰊄", hl = "Type" },
			StaticMethod = { icon = '󰠄 ', hl = 'Function' },
		}
	},
	-- Add this section only if you've configured source selector.
	source_selector = {
		sources = {
			{ source = "filesystem", display_name = " 󰉓 Files " },
			{ source = "git_status", display_name = " 󰊢 Git " },
		},
	},
	close_if_last_window = true, -- Close Neo-tree if it is the last window left in the tab
	filesystem = {
		filtered_items = {
			hide_dotfiles = false,
		},
	},
})
