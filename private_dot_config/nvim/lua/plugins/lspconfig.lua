return {
  "neovim/nvim-lspconfig",
  opts = {
    inlay_hints = {
      enabled = true,
    },
    setup = {
      -- fix warning with rustaceans.nvim https://github.com/mrcjkb/rustaceanvim/blob/master/doc/mason.txt
      rust_analyzer = function()
        return true
      end,
      clangd = function(_, opts)
        opts.capabilities.offsetEncoding = { "utf-16" }
      end,
    },
  },
  init = function()
    local keys = require("lazyvim.plugins.lsp.keymaps").get()
    -- disable a keymap
    keys[#keys + 1] = { "<leader>cc", false }
    keys[#keys + 1] = { "<leader>cC", false }
  end,
}
