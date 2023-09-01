return {
  {
    "neovim/nvim-lspconfig",
    opts = {
      setup = {
        clangd = function(_, opts)
          opts.capabilities.offsetEncoding = { "utf-16" }
        end,
      },
    },
  },
  {
    "p00f/clangd_extensions.nvim",
    opts = {
      inlay_hints = {
        inline = true,
      },
    },
  },
}
