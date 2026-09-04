local Event = require("lazy.core.handler.event")

local mapping = { id = "LazyFile", event = "User", pattern = "LazyFile" }
Event.mappings.LazyFile = mapping
Event.mappings["User LazyFile"] = mapping

local autocmd
autocmd = vim.api.nvim_create_autocmd({ "BufReadPost", "FileType", "BufWritePost" }, {
	callback = function(ev)
		if vim.bo[ev.buf].buftype ~= "" or vim.api.nvim_buf_get_name(ev.buf):find("://", 1, true) then
			return
		end
		if
			ev.event ~= "BufWritePost"
			and vim.api.nvim_buf_line_count(ev.buf) == 1
			and vim.api.nvim_buf_get_lines(ev.buf, 0, 1, false)[1] == ""
		then
			return
		end

		local state = Event.get_state(ev.event, ev.buf, ev.data)
		if ev.event == "BufWritePost" then
			state = vim.list_extend(Event.get_state("FileType", ev.buf), state)
		end
		vim.api.nvim_del_autocmd(autocmd)
		vim.api.nvim_buf_call(ev.buf, function()
			vim.api.nvim_exec_autocmds("User", { pattern = "LazyFile", modeline = false })
		end)
		for _, event in ipairs(state) do
			Event.trigger(event)
		end
	end,
})

return {
	{
		"neovim/nvim-lspconfig",
		event = function()
			return { "LazyFile" }
		end,
		opts = {
			inlay_hints = {
				enabled = true,
				exclude = {},
			},
		},
	},
}
