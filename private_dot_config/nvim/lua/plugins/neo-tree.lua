return {
  "nvim-neo-tree/neo-tree.nvim",
  opts = {
    close_if_last_window = true,
    filesystem = {
      hijack_netrw_behavior = "open_default", -- netrw disabled, opening a directory opens neo-tree
    },
  },
}
