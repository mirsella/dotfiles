return {
  "hrsh7th/nvim-cmp",
  event = "InsertEnter",
  lazy = true,
  dependencies = {
    { "petertriho/cmp-git", lazy = true },
    "nvim-lua/plenary.nvim",
  },
  opts = function(_, opts)
    local cmp = require("cmp")
    table.insert(opts.sources, {
      { name = "git" },
      {
        -- name = "buffer",
        option = {
          get_bufnrs = function()
            return vim.api.nvim_list_bufs()
          end,
        },
      },
    })
    opts.window = {
      completion = cmp.config.window.bordered(),
      documentation = cmp.config.window.bordered(),
    }
    -- opts.formatting = {
    --   format = function(_, item)
    --     local icons = require("lazyvim.config").icons.kinds
    --     if icons[item.kind] then
    --       item.kind = icons[item.kind] .. item.kind
    --     end
    --     item.abbr = string.sub(item.abbr, 1, vim.api.nvim_win_get_width(0) / 3)
    --     if item.menu then
    --       item.menu = string.sub(item.menu, 1, vim.api.nvim_win_get_width(0) / 3)
    --     end
    --     return item
    --   end,
    -- }
  end,
}
