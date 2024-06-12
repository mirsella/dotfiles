-- local function set_colorscheme_based_on_time()
--   local hour = tonumber(os.date("%H"))
--   if hour >= 23 or hour < 7 then
--     vim.cmd("colorscheme rose-pine-main")
--   elseif hour >= 18 then
--     vim.cmd("colorscheme rose-pine-moon")
--   else
--     vim.cmd("colorscheme rose-pine-dawn")
--   end
-- end
-- set_colorscheme_based_on_time()
-- local timer = vim.loop.new_timer()
-- timer:start(0, 300000, vim.schedule_wrap(set_colorscheme_based_on_time))

return {
	{
		"rose-pine/neovim",
		priority = 10000,
	},

	{
		"LazyVim/LazyVim",
		opts = { colorscheme = function() end },
	},
	{ "folke/tokyonight.nvim", enabled = false },
	{ "catppuccin", enabled = false },
}
