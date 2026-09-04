local autocmd
autocmd = vim.api.nvim_create_autocmd({ "BufAdd", "BufEnter" }, {
	callback = function()
		local listed = 0
		for _, buf in ipairs(vim.api.nvim_list_bufs()) do
			listed = listed + (vim.bo[buf].buflisted and 1 or 0)
		end
		if listed > 1 then
			vim.api.nvim_del_autocmd(autocmd)
			vim.api.nvim_exec_autocmds("User", { pattern = "MultipleBuffers", modeline = false })
		end
	end,
})

return {
	"akinsho/bufferline.nvim",
	event = function()
		return { "User MultipleBuffers" }
	end,
}
