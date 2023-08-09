local Util = require("lazyvim.util")

return {
  "nvim-telescope/telescope.nvim",
  dependencies = {
    "nvim-lua/plenary.nvim",
    "debugloop/telescope-undo.nvim",
  },
  keys = {
    { "<leader>uu", "<cmd>Telescope undo<cr>", desc = "Telescope undo" },

    { "<leader><space>", Util.telescope("files", { hidden = true }), desc = "Find Files (root dir)" },
    { "<leader>ff", Util.telescope("files", { hidden = true }), desc = "Find Files (root dir)" },
    { "<leader>fF", Util.telescope("files", { cwd = false, hidden = true }), desc = "Find Files (cwd)" },
  },
  config = function()
    local telescope = require("telescope")
    telescope.load_extension("undo")
  end,
}
