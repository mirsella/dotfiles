return {
  {
    "nvim-neo-tree/neo-tree.nvim",
    enabled = false,
  },
  {
    "stevearc/oil.nvim",
    dependencies = { "nvim-tree/nvim-web-devicons" },
    opts = {
      delete_to_trash = true,
    },
    lazy = false,
    keys = {
      { "<leader>e", "<CMD>Oil<CR>", desc = "Open parent directory" },
      { "-", "<CMD>Oil<CR>", desc = "Open parent directory" },
    },
  },
}
