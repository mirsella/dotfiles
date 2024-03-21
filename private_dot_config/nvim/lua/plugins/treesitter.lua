return {
  "nvim-treesitter/nvim-treesitter",
  opts = {
    auto_install = true,
    opts = {
      ensure_installed = {
        "bash",
        "html",
        "javascript",
        "json",
        "lua",
        "markdown",
        "markdown_inline",
        "python",
        "query",
        "regex",
        "tsx",
        "typescript",
        "vim",
        "yaml",
        "rust",
        "vue",
        "toml",
      },
    },
  },
}
