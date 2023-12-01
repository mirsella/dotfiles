-- Keymaps are automatically loaded on the VeryLazy event
-- Default keymaps that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/keymaps.lua
-- Add any additional keymaps here

local opts = { silent = true }

vim.api.nvim_create_user_command("W", "w !sudo tee %", {})

vim.keymap.set("n", "U", "<cmd>echo '< < ===== C H E C K   C A P S   L O C K ===== > >'<cr>", opts)
vim.keymap.set("i", "<c-t>", "<esc>", opts)
vim.keymap.set("n", "<leader>sf", "<cmd>Telescope filetypes<cr>", opts)
vim.keymap.set({ "n", "v" }, "<leader>aa", vim.lsp.buf.code_action, opts)

local crates = require("crates")
vim.keymap.set("n", "<leader>cf", crates.show_features_popup, opts)
