return {
  "simrat39/rust-tools.nvim",
  keys = {
    {
      "<leader>cc",
      "<cmd>lua require'rust-tools'.open_cargo_toml.open_cargo_toml()<cr>",
      desc = "open Cargo.toml",
      silent = true,
      noremap = true,
    },
  },
}
