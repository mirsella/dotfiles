local builtin = require("telescope.builtin")
local utils = require("telescope.utils")
return {
  "nvim-telescope/telescope.nvim",
  keys = {
    {
      "<leader>ff",
      function()
        builtin.find_files()
      end,
      desc = "Find Files (root dir)",
    },
    {
      "<leader>fF",
      function()
        builtin.find_files({ cwd = utils.buffer_dir() })
      end,
      desc = "Find Files (cwd)",
    },
    { "<leader><space>", false },
    {
      "<leader>ft",
      function()
        builtin.filetype()
      end,
      { silent = true, desc = "Change filetype with Telescope" },
    },
  },
}
