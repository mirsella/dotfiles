return {
  "neovim/nvim-lspconfig",
  opts = {
    inlay_hints = {
      enabled = true,
    },
    -- for vue-language-server version 2
    servers = {
      tsserver = {
        init_options = {
          plugins = {
            {
              name = "@vue/typescript-plugin",
              location = "/usr/lib/node_modules/@vue/typescript-plugin",
              languages = { "javascript", "typescript", "vue" },
            },
          },
        },
        filetypes = { "typescript", "javascript", "vue" },
      },
      volar = {},
    },
  },
}
