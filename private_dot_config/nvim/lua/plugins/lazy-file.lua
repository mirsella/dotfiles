local Event = require("lazy.core.handler.event")

local function is_file(buf)
	local name = vim.api.nvim_buf_get_name(buf)
	return vim.bo[buf].buftype == "" and name ~= "" and not name:match("^%a[%w+.-]*://")
end

local mapping = { id = "LazyFile", event = "User", pattern = "LazyFile" }
Event.mappings.LazyFile = mapping
Event.mappings["User LazyFile"] = mapping

local autocmd
autocmd = vim.api.nvim_create_autocmd({ "BufReadPost", "FileType", "BufWritePost" }, {
	callback = function(ev)
		if not is_file(ev.buf) or (ev.event == "BufReadPost" and not vim.bo[ev.buf].buflisted) then
			return
		end

		local empty = vim.api.nvim_buf_line_count(ev.buf) == 1
			and vim.api.nvim_buf_get_lines(ev.buf, 0, 1, false)[1] == ""
		if ev.event ~= "BufWritePost" and empty then
			if ev.event == "FileType" and not package.loaded["nvim-treesitter"] then
				local lang = vim.treesitter.language.get_lang(ev.match)
				if lang and vim.treesitter.language.add(lang) and vim.treesitter.query.get(lang, "highlights") then
					require("lazy").load({ plugins = { "nvim-treesitter" } })
					vim.api.nvim_exec_autocmds("FileType", { buffer = ev.buf, modeline = false })
				end
			end
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
	{
		"nvim-treesitter/nvim-treesitter",
		event = function()
			return { "LazyFile" }
		end,
		opts = { auto_install = true },
	},
}
