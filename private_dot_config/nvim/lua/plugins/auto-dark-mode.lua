return {
  "f-person/auto-dark-mode.nvim",
  lazy = false,
  priority = 1000,
  config = {
    update_interval = 1000,
    set_dark_mode = function()
      vim.api.nvim_set_option("background", "dark")
      vim.cmd("colorscheme rose-pine-moon")
    end,
    set_light_mode = function()
      vim.api.nvim_set_option("background", "light")
      vim.cmd("colorscheme rose-pine")
    end,
  },
}
