return {
  {
    "nvim-neo-tree/neo-tree.nvim",
    enabled = false,
  },
  {
    "stevearc/oil.nvim",
    opts = {
      delete_to_trash = true,
    },
    dependencies = { "nvim-tree/nvim-web-devicons" },
  },
}
