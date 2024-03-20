return {
  "gbprod/cutlass.nvim",
  event = { "LazyFile" },
  opts = {
    cut_key = "<leader>d",
    override_del = true,
    registers = {
      select = "s",
      delete = "d",
      change = "c",
    },
  },
}
