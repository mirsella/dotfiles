-- Keymaps are automatically loaded on the VeryLazy event
-- Default keymaps that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/keymaps.lua
-- Add any additional keymaps here

vim.api.nvim_create_user_command("W", "w !sudo tee %", {})

vim.keymap.set("n", "U", "<cmd>echo '< < ===== C H E C K   C A P S   L O C K ===== > >'<cr>")
vim.keymap.set("i", "<c-t>", "<esc>")
vim.keymap.set("n", "<leader>sf", "<cmd>Telescope filetypes<cr>")

local crates = require("crates")

vim.keymap.set(
  "n",
  "<leader>cv",
  crates.show_versions_popup,
  { silent = true, noremap = true, desc = "show crates versions" }
)
vim.keymap.set(
  "n",
  "<leader>cf",
  crates.show_features_popup,
  { silent = true, noremap = true, desc = "show crates features" }
)
vim.keymap.set(
  "n",
  "<leader>cd",
  crates.show_dependencies_popup,
  { silent = true, noremap = true, desc = "show crates dependencies" }
)

vim.keymap.set("n", "<leader>cu", crates.update_crate, { silent = true, noremap = true, desc = "update crate" })
vim.keymap.set("v", "<leader>cu", crates.update_crates, { silent = true, noremap = true, desc = "update crates" })
vim.keymap.set("n", "<leader>cU", crates.upgrade_crate, { silent = true, noremap = true, desc = "upgrade crate" })
vim.keymap.set("v", "<leader>cU", crates.upgrade_crates, { silent = true, noremap = true, desc = "upgrade crates" })

vim.keymap.set(
  "n",
  "<leader>ce",
  crates.expand_plain_crate_to_inline_table,
  { silent = true, noremap = true, desc = "expand crate to inline table" }
)
vim.keymap.set(
  "n",
  "<leader>cE",
  crates.extract_crate_into_table,
  { silent = true, noremap = true, desc = "expand crate into table" }
)

vim.keymap.set("n", "<leader>cH", crates.open_homepage, { silent = true, noremap = true, desc = "open crate homepage" })
vim.keymap.set(
  "n",
  "<leader>cR",
  crates.open_repository,
  { silent = true, noremap = true, desc = "open crate repository" }
)
vim.keymap.set(
  "n",
  "<leader>cD",
  crates.open_documentation,
  { silent = true, noremap = true, desc = "open crate homepage" }
)
vim.keymap.set(
  "n",
  "<leader>cC",
  crates.open_crates_io,
  { silent = true, noremap = true, desc = "open crate on crates.io" }
)
