-- Keymaps are automatically loaded on the VeryLazy event
-- Default keymaps that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/keymaps.lua
-- Add any additional keymaps here

vim.api.nvim_create_user_command("W", "w !sudo tee %", {})

vim.keymap.set(
  "n",
  "U",
  "<cmd>echo '< < ===== C H E C K   C A P S   L O C K ===== > >'<cr>",
  { desc = "Check Caps Lock" }
)
vim.keymap.set("i", "<c-t>", "<esc>", { silent = true })
vim.keymap.set(
  "n",
  "<leader>ft",
  "<cmd>Telescope filetypes<cr>",
  { silent = true, desc = "Change filetype with Telescope" }
)
vim.keymap.set({ "n", "v" }, "<leader><leader>", vim.lsp.buf.code_action, { silent = true, desc = "Show code actions" })

local crates = require("crates")
vim.keymap.set(
  "n",
  "<leader>cf",
  crates.show_features_popup,
  { silent = true, desc = "Show features popup in Cargo.toml" }
)

vim.keymap.set("n", "'", "`", { desc = "Go to mark" })
