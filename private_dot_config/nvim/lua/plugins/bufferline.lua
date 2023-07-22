require('bufferline').setup {
  options = {
    hover = {
      enabled = true,
      delay = 0,
      reveal = {'close'}
    },
    numbers = 'buffer_id',
    indicator = {
      style = 'none'
    },
    diagnostics = 'nvim_lsp',
    diagnostics_indicator = function(count, level, diagnostics_dict, context)
      local s = ''
      for e, n in pairs(diagnostics_dict) do
        local sym = e == 'error' and ' '
        or (e == 'warning' and ' ' or ' ')
        s = s .. n .. sym
      end
      return s
    end,
    tab_size = 0,
    diagnostics_update_in_insert = true,
    show_tab_indicators = false,
  }
}
