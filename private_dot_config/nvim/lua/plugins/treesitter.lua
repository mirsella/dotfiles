require('nvim-treesitter.configs').setup {
  context_commentstring = {
    enable = true
  },
  -- ensure_installed = { "c", "cpp", "lua", "vim", "vue", "typescript", "javascript", "json", "html", "css", "bash", "python", "rust", "java", "regex" },
  -- Automatically install missing parsers when entering buffer
  -- Recommendation: set to false if you don't have `tree-sitter` CLI installed locally
  auto_install = true,
  highlight = {
    enable = true,
    additional_vim_regex_highlighting = false,
  },
  indent = {
    enable = true,
    -- disable = { "python", "rust" },
  },
}
