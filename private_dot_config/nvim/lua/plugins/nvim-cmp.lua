return {
  "hrsh7th/nvim-cmp",
  event = "InsertEnter",
  lazy = true,
  dependencies = {
    { "petertriho/cmp-git", lazy = true },
    -- {
    --   "jcdickinson/codeium.nvim",
    --   opts = {},
    --   config = function(_, opts)
    --     require("codeium").setup(opts)
    --   end,
    -- },
    "nvim-lua/plenary.nvim",
  },
  opts = function(_, opts)
    local cmp = require("cmp")
    opts.sources = cmp.config.sources(vim.list_extend(opts.sources, {
      { name = "git" },
      { name = "copilot", group_index = 0, priority = 100 },
      -- { name = "codeium", priority = 100 },
      {
        name = "buffer",
        option = {
          get_bufnrs = function()
            return vim.api.nvim_list_bufs()
          end,
        },
      },
    }))
    opts.window = {
      completion = cmp.config.window.bordered(),
      documentation = cmp.config.window.bordered(),
    }
    opts.formatting = {
      format = function(_, item)
        local icons = require("lazyvim.config").icons.kinds
        if icons[item.kind] then
          item.kind = icons[item.kind] .. item.kind
        end
        item.abbr = string.sub(item.abbr, 1, vim.api.nvim_win_get_width(0) / 3)
        if item.menu then
          item.menu = string.sub(item.menu, 1, vim.api.nvim_win_get_width(0) / 3)
        end
        return item
      end,
    }
  end,
}
