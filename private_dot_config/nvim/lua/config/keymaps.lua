-- Keymaps are automatically loaded on the VeryLazy event
-- Default keymaps that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/keymaps.lua
-- Add any additional keymaps here

vim.api.nvim_create_user_command("W", "w !sudo tee %", {})

vim.keymap.set(
	"n",
	"U",
	"<cmd>echo '< < ===== C H E C K   C A P S   L O C K ===== > >'<cr>",
	{ desc = "Check Caps Lock" }
)
vim.keymap.set("i", "<c-t>", "<esc>", { silent = true })
vim.keymap.set(
	{ "n", "v" },
	"<leader><leader>",
	vim.lsp.buf.code_action,
	{ silent = true, desc = "Show code actions", noremap = true }
)
vim.keymap.set("n", "'", "`", { desc = "Go to mark" })

vim.keymap.set("n", "<leader>fp", function()
	local path = vim.fn.expand("%:p")
	vim.fn.setreg("+", path)
	vim.notify("Copied: " .. path)
end, { desc = "Copy current buffer full path to clipboard" })

vim.keymap.set("n", "<LeftMouse>", function()
	local mouse = vim.fn.getmousepos()

	-- Return early if click is not in a valid window (e.g., command line)
	if mouse.winid <= 0 then
		return "<LeftMouse>"
	end

	-- If clicking into a different window, focus it and suppress the click action
	if mouse.winid ~= vim.api.nvim_get_current_win() then
		vim.api.nvim_set_current_win(mouse.winid)
		return ""
	end

	-- If already in the window, allow default behavior (moving cursor/toggling folds)
	return "<LeftMouse>"
end, { expr = true, desc = "Focus window without moving cursor on initial click" })
