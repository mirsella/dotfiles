return {
  "mrcjkb/rustaceanvim",
  keys = {
    {
      "n",
      "<leader>cc",
      function()
        vim.cmd.RustLsp("openCargo")
      end,
      { desc = "Open Cargo.toml" },
    },
    {
      "n",
      "<leader>cC",
      function()
        vim.cmd.RustLsp("openDocs")
      end,
      { desc = "Open docs.rs document for symbol" },
    },
  },
}
