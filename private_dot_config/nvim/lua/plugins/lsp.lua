local capabilities = require('cmp_nvim_lsp').default_capabilities()

require('mason').setup()
require('mason-lspconfig').setup {
  ensure_installed = { 'pyright', 'lua_ls', 'tsserver', 'bashls', 'jdtls', 'tailwindcss', 'vimls', 'volar', 'clangd', 'rust_analyzer', 'emmet_ls'},
  automatic_installation = true,
}
require("mason-lspconfig").setup_handlers {
  -- The first entry (without a key) will be the default handler
  -- and will be called for each installed server that doesn't have
  -- a dedicated handler.
  function (server_name) -- default handler (optional)
    require("lspconfig")[server_name].setup {
      capabilities = capabilities,
    }
  end,
  -- Next, you can provide a dedicated handler for specific servers.
  -- For example, a handler override for the `rust_analyzer`:
  ["rust_analyzer"] = function ()
    require("rust-tools").setup {
      capabilities = capabilities,
      server = {
        settings = {
          ["rust-analyzer"] = {
            -- checkOnSave = {
            -- 	command = "clippy",
            -- },
            checkOnSave = {
              allFeatures = true,
              overrideCommand = {
                'cargo', 'clippy', '--workspace', '--message-format=json',
                '--all-targets', '--all-features'
              }
            }
          },
        },
      }
    }
  end,
  ["lua_ls"] = function ()
    require('lspconfig').lua_ls.setup {
      capabilities = capabilities,
      settings = {
        Lua = {
          diagnostics = {
            globals = { "vim" }
          }
        }
      }
    }
  end,
  ["volar"] = function ()
    require'lspconfig'.volar.setup {
      capabilities = capabilities,
      filetypes = {'typescript', 'javascript', 'javascriptreact', 'typescriptreact', 'vue', 'json'}
      -- filetypes = {'vue'}
    }
  end
}
