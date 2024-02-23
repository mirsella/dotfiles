local Util = require("util")
return {
  "nvim-telescope/telescope.nvim",
  keys = {
    { "<leader>ff", Util.telescope("files"), desc = "Find Files (root dir)" },
    { "<leader>fF", Util.telescope("files", { cwd = false }), desc = "Find Files (cwd)" },
    { "<leader><space>", false },
    { "<leader>ft", false },
  },
}
