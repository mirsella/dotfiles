-- Options are automatically loaded before lazy.nvim startup
-- Default options that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/options.lua
-- Add any additional options here

-- vim.opt.pumblend = 0 -- opaque completion menu
-- vim.opt.softtabstop = 2
vim.opt.foldmethod = "expr"
vim.opt.foldexpr = "v:lua.vim.treesitter.foldexpr()"
vim.opt.foldlevel = 99
-- vim.opt.spelllang = "fr"
-- vim.g.lazyvim_rust_diagnostics = "bacon-ls"
vim.g.lazyvim_picker = "fzf"
