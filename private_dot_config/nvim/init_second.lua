require('impatient')
require('hop').setup()
require('nvim-web-devicons').setup()
require('colorizer').setup()
require('telescope').setup()
require('cloak').setup()
require("trouble").setup()

require("plugins.cmp")
require("plugins.lsp")
require("plugins.copilot")
require("plugins.lualine")
require("plugins.bufferline")
require("plugins.neotree")
require("plugins.treesitter")

vim.keymap.set("n", "gd", function() vim.lsp.buf.definition() end)
vim.keymap.set("n", "<leader>ca", function() vim.lsp.buf.code_action() end)
