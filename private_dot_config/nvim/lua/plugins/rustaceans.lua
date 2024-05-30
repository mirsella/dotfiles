return {
  "mrcjkb/rustaceanvim",
  keys = {
    {
      "<leader>cc",
      function()
        vim.cmd.RustLsp("openCargo")
      end,
      { desc = "Open Cargo.toml" },
    },
    {
      "<leader>cC",
      function()
        vim.cmd.RustLsp("openDocs")
      end,
      { desc = "Open docs.rs document for symbol" },
    },
  },
}
