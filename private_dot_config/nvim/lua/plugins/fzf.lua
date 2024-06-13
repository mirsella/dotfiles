local function buf_dir()
  local num = vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(num)
  return vim.uv.fs_realpath(name)
end

return {
  {
    "ibhagwan/fzf-lua",
    keys = {
      {
        "<leader>ff",
        function()
          require("fzf-lua").files()
        end,
        desc = "Find Files (Root Dir)",
      },
      {
        "<leader>fF",
        function()
          require("fzf-lua").files({ cwd = buf_dir() })
        end,
        desc = "Find Files (Buffer Dir)",
      },
    },
  },
  {
    "nvim-telescope/telescope.nvim",
    enabled = false,
  },
}
