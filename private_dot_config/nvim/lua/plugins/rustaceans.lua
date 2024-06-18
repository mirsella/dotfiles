return {
  "mrcjkb/rustaceanvim",
  keys = {
    {
      "<leader>cc",
      function()
        vim.cmd.RustLsp("openCargo")
      end,
      ft = "rust",
      desc = "Open Cargo.toml",
    },
    {
      "<leader>cC",
      function()
        vim.cmd.RustLsp("openDocs")
      end,
      ft = "rust",
      desc = "Open docs.rs document for symbol"
    },
  },
}
