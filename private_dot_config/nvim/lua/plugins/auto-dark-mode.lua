return {
	"f-person/auto-dark-mode.nvim",
	event = { "CursorHold", "CursorHoldI" },
	init = function()
		local result = vim.system({
			"dbus-send",
			"--session",
			"--print-reply=literal",
			"--reply-timeout=1000",
			"--dest=org.freedesktop.portal.Desktop",
			"/org/freedesktop/portal/desktop",
			"org.freedesktop.portal.Settings.Read",
			"string:org.freedesktop.appearance",
			"string:color-scheme",
		}, { text = true }):wait(1000)
		if result.code ~= 0 then
			vim.schedule(function()
				vim.notify("Could not detect the system color scheme", vim.log.levels.WARN)
			end)
			return
		end
		vim.o.background = result.stdout:find("uint32 1", 1, true) and "dark" or "light"
	end,
	opts = {
		update_interval = 1000,
		set_dark_mode = function()
			if vim.g.colors_name ~= "rose-pine" or vim.o.background ~= "dark" then
				vim.cmd.colorscheme("rose-pine-moon")
			end
		end,
		set_light_mode = function()
			if vim.g.colors_name ~= "rose-pine" or vim.o.background ~= "light" then
				vim.cmd.colorscheme("rose-pine-dawn")
			end
		end,
	},
}
