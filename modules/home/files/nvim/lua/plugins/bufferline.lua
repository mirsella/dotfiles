local autocmd
autocmd = vim.api.nvim_create_autocmd({ "BufAdd", "BufEnter" }, {
	callback = function()
		if #vim.fn.getbufinfo({ buflisted = 1 }) > 1 then
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
