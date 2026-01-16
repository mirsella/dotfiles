return {
	{
		"nvim-mini/mini.hipatterns",
		opts = function(_, opts)
			local hipatterns = require("mini.hipatterns")
			local uv = vim.uv or vim.loop

			opts.highlighters = opts.highlighters or {}

			local function is_rust(buf_id)
				return vim.bo[buf_id].filetype == "rust"
			end

			local function dbg_enabled()
				return vim.g.bevy_hipatterns_debug == true or vim.g.bevy_hipatterns_debug == 1
			end

			local function dbg(msg, level)
				if not dbg_enabled() then
					return
				end
				vim.schedule(function()
					vim.notify(("[bevy-hipatterns] %s"):format(msg), level or vim.log.levels.INFO)
				end)
			end

			local function clamp(x, lo, hi)
				if x < lo then
					return lo
				end
				if x > hi then
					return hi
				end
				return x
			end

			local function clamp_u8(x)
				return clamp(x, 0, 255)
			end

			local function rgb_u8_to_hex(r, g, b)
				r, g, b = clamp_u8(r), clamp_u8(g), clamp_u8(b)
				return string.format("#%02X%02X%02X", r, g, b)
			end

			local function hex_to_rgb_u8(hex)
				local h = hex:gsub("^#", "")
				if #h ~= 6 then
					return nil
				end
				local r = tonumber(h:sub(1, 2), 16)
				local g = tonumber(h:sub(3, 4), 16)
				local b = tonumber(h:sub(5, 6), 16)
				if not r or not g or not b then
					return nil
				end
				return r, g, b
			end

			local function get_normal_bg_hex()
				local ok, hl = pcall(vim.api.nvim_get_hl, 0, {
					name = "Normal",
					link = false,
				})
				if ok and type(hl) == "table" and type(hl.bg) == "number" then
					return string.format("#%06X", hl.bg)
				end
				return "#000000"
			end

			local function blend_hex_over_bg(hex_fg, hex_bg, alpha)
				alpha = clamp(alpha or 1, 0, 1)

				local fr, fg, fb = hex_to_rgb_u8(hex_fg)
				local br, bg, bb = hex_to_rgb_u8(hex_bg)
				if not fr or not br then
					return hex_fg
				end

				local function mix(f, b)
					return math.floor(f * alpha + b * (1 - alpha) + 0.5)
				end

				return rgb_u8_to_hex(mix(fr, br), mix(fg, bg), mix(fb, bb))
			end

			local function srgb_float_to_u8(x)
				x = clamp(x, 0, 1)
				return math.floor(x * 255 + 0.5)
			end

			local function group_from_hex(hex)
				return hipatterns.compute_hex_color_group(hex, "bg")
			end

			local function parse_num_prefix(s)
				local n = s:match("[%+%-]?%d*%.?%d+")
				return n and tonumber(n) or nil
			end

			local function parse_u8_prefix(s)
				local n = s:match("%d+")
				return n and tonumber(n) or nil
			end

			local function parse_ctor_args(match, ctor_pat)
				local inside = match:match(ctor_pat .. "%s*%((.*)%)")
				if not inside then
					return nil
				end
				local parts = vim.split(inside, ",", { plain = true, trimempty = true })
				for i, v in ipairs(parts) do
					parts[i] = vim.trim(v)
				end
				return parts
			end

			local function mk_color_u8_highlighter(key, ctor_pat, argc)
				opts.highlighters[key] = {
					pattern = ctor_pat .. "%s*%b()",
					group = function(buf_id, match)
						if not is_rust(buf_id) then
							return nil
						end

						local args = parse_ctor_args(match, ctor_pat)
						if not args or #args < argc then
							return nil
						end

						local r = parse_u8_prefix(args[1])
						local g = parse_u8_prefix(args[2])
						local b = parse_u8_prefix(args[3])
						if r == nil or g == nil or b == nil then
							return nil
						end

						local a = 1
						if argc == 4 then
							local aa = parse_u8_prefix(args[4])
							if aa == nil then
								return nil
							end
							a = clamp(aa / 255, 0, 1)
						end

						local hex = rgb_u8_to_hex(r, g, b)
						if a < 1 then
							hex = blend_hex_over_bg(hex, get_normal_bg_hex(), a)
						end

						return group_from_hex(hex)
					end,
				}
			end

			local function mk_color_float_highlighter(key, ctor_pat, argc)
				opts.highlighters[key] = {
					pattern = ctor_pat .. "%s*%b()",
					group = function(buf_id, match)
						if not is_rust(buf_id) then
							return nil
						end

						local args = parse_ctor_args(match, ctor_pat)
						if not args or #args < argc then
							return nil
						end

						local r = parse_num_prefix(args[1])
						local g = parse_num_prefix(args[2])
						local b = parse_num_prefix(args[3])
						if r == nil or g == nil or b == nil then
							return nil
						end

						local a = 1
						if argc == 4 then
							local aa = parse_num_prefix(args[4])
							if aa == nil then
								return nil
							end
							a = clamp(aa, 0, 1)
						end

						local hex = rgb_u8_to_hex(srgb_float_to_u8(r), srgb_float_to_u8(g), srgb_float_to_u8(b))

						if a < 1 then
							hex = blend_hex_over_bg(hex, get_normal_bg_hex(), a)
						end

						return group_from_hex(hex)
					end,
				}
			end

			-- Bevy Color constructors (u8 + float)
			mk_color_u8_highlighter("bevy_color_srgb_u8", "Color::srgb_u8", 3)
			mk_color_u8_highlighter("bevy_color_rgb_u8", "Color::rgb_u8", 3)
			mk_color_u8_highlighter("bevy_color_srgba_u8", "Color::srgba_u8", 4)
			mk_color_u8_highlighter("bevy_color_rgba_u8", "Color::rgba_u8", 4)

			mk_color_float_highlighter("bevy_color_srgb", "Color::srgb", 3)
			mk_color_float_highlighter("bevy_color_rgb", "Color::rgb", 3)
			mk_color_float_highlighter("bevy_color_srgba", "Color::srgba", 4)
			mk_color_float_highlighter("bevy_color_rgba", "Color::rgba", 4)

			-- Palettes: tailwind::NEUTRAL_500, css::ALICE_BLUE, etc.
			local palettes_loaded = false
			local palettes = { tailwind = {}, css = {} }
			local miss_logged = {}

			local function cargo_home()
				local ch = vim.env.CARGO_HOME
				if ch and ch ~= "" then
					return ch
				end

				local xdg_data = vim.env.XDG_DATA_HOME
				if xdg_data and xdg_data ~= "" then
					return xdg_data .. "/cargo"
				end

				local home = uv.os_homedir() or vim.fn.expand("~")
				return home .. "/.cargo"
			end

			local function glob_list(pattern)
				local res = vim.fn.glob(pattern, true, true)
				if type(res) == "string" then
					if res == "" then
						return {}
					end
					return { res }
				end
				return res or {}
			end

			local function dedup(list)
				local out = {}
				local seen = {}
				for _, p in ipairs(list) do
					if not seen[p] then
						seen[p] = true
						out[#out + 1] = p
					end
				end
				return out
			end

			local function newest_by_mtime(paths)
				local best_path = nil
				local best_mtime = -1

				for _, p in ipairs(paths) do
					local st = uv.fs_stat(p)
					local m = st and st.mtime and st.mtime.sec or -1
					if m > best_mtime then
						best_mtime = m
						best_path = p
					end
				end

				return best_path
			end

			local function find_palette_files(filename)
				local ch = cargo_home()
				local patterns = {
					ch .. "/registry/src/*/bevy_color-*/src/palettes/" .. filename,
					ch .. "/git/checkouts/*bevy*/*/crates/bevy_color/src/palettes/" .. filename,
				}

				local matches = {}
				for _, pat in ipairs(patterns) do
					for _, p in ipairs(glob_list(pat)) do
						matches[#matches + 1] = p
					end
				end

				matches = dedup(matches)
				table.sort(matches, function(a, b)
					local sa = uv.fs_stat(a)
					local sb = uv.fs_stat(b)
					local ma = sa and sa.mtime and sa.mtime.sec or 0
					local mb = sb and sb.mtime and sb.mtime.sec or 0
					return ma > mb
				end)

				return matches
			end

			local function read_file(path)
				local ok, lines = pcall(vim.fn.readfile, path)
				if not ok or not lines then
					return nil
				end
				return table.concat(lines, "\n")
			end

			local function extract_hex_literal(expr)
				local h8 = expr:match('"#?(%x%x%x%x%x%x%x%x)"')
				if h8 then
					return h8:upper()
				end
				local h6 = expr:match('"#?(%x%x%x%x%x%x)"')
				if h6 then
					return h6:upper()
				end

				local rh8 = expr:match('r#*"#?(%x%x%x%x%x%x%x%x)"#*')
				if rh8 then
					return rh8:upper()
				end
				local rh6 = expr:match('r#*"#?(%x%x%x%x%x%x)"#*')
				if rh6 then
					return rh6:upper()
				end

				return nil
			end

			local function split_args(arg_str)
				local parts = vim.split(arg_str, ",", { plain = true, trimempty = true })
				for i, v in ipairs(parts) do
					parts[i] = vim.trim(v)
				end
				return parts
			end

			local function parse_palette_expr(expr)
				-- Prefer hex anywhere in RHS.
				local hex = extract_hex_literal(expr)
				if hex then
					local out = { hex = "#" .. hex:sub(1, 6), a = 1 }
					if #hex == 8 then
						local a_u8 = tonumber(hex:sub(7, 8), 16)
						out.a = a_u8 and clamp(a_u8 / 255, 0, 1) or 1
					end
					return out
				end

				-- Fallback: numeric ctors like Srgba::new(...), Srgba::rgb_u8(...), etc.
				local ctor, args = expr:match("::([%w_]+)%s*(%b())")
				if not ctor or not args then
					return nil
				end

				local parts = split_args(args:sub(2, -2))
				local r_u8, g_u8, b_u8
				local a = 1

				if (ctor == "rgb_u8" or ctor == "srgb_u8") and #parts >= 3 then
					r_u8 = parse_u8_prefix(parts[1])
					g_u8 = parse_u8_prefix(parts[2])
					b_u8 = parse_u8_prefix(parts[3])
				elseif (ctor == "rgba_u8" or ctor == "srgba_u8") and #parts >= 4 then
					r_u8 = parse_u8_prefix(parts[1])
					g_u8 = parse_u8_prefix(parts[2])
					b_u8 = parse_u8_prefix(parts[3])
					local a_u8 = parse_u8_prefix(parts[4])
					a = a_u8 and clamp(a_u8 / 255, 0, 1) or 1
				elseif (ctor == "rgb" or ctor == "srgb") and #parts >= 3 then
					local r = parse_num_prefix(parts[1])
					local g = parse_num_prefix(parts[2])
					local b = parse_num_prefix(parts[3])
					if r and g and b then
						r_u8 = srgb_float_to_u8(r)
						g_u8 = srgb_float_to_u8(g)
						b_u8 = srgb_float_to_u8(b)
					end
				elseif
					(ctor == "rgba" or ctor == "srgba" or ctor == "new" or ctor == "new_unchecked")
					and #parts >= 4
				then
					local r = parse_num_prefix(parts[1])
					local g = parse_num_prefix(parts[2])
					local b = parse_num_prefix(parts[3])
					local aa = parse_num_prefix(parts[4])
					if r and g and b and aa then
						r_u8 = srgb_float_to_u8(r)
						g_u8 = srgb_float_to_u8(g)
						b_u8 = srgb_float_to_u8(b)
						a = clamp(aa, 0, 1)
					end
				end

				if r_u8 and g_u8 and b_u8 then
					return { hex = rgb_u8_to_hex(r_u8, g_u8, b_u8), a = a }
				end

				return nil
			end

			local function add_palette_from_content(dst, content)
				local added = 0

				-- [%s%S] to match across newlines. Stops at first semicolon.
				for name, expr in content:gmatch("pub%s+const%s+([%u%d_]+)%s*:%s*[%w_:<>]+%s*=%s*([%s%S]-)%s*;") do
					local entry = parse_palette_expr(expr)
					if entry then
						dst[name] = entry
						added = added + 1
					end
				end

				return added
			end

			local function load_palettes_once()
				if palettes_loaded then
					return
				end
				palettes_loaded = true

				local ch = cargo_home()
				dbg(("cargo_home=%s"):format(ch))

				local function load_mod(mod, filename)
					local files = find_palette_files(filename)
					dbg(("%s: found %d candidate file(s) for %s"):format(mod, #files, filename))

					local total_added = 0
					for _, path in ipairs(files) do
						local content = read_file(path)
						if content then
							local added = add_palette_from_content(palettes[mod], content)
							total_added = total_added + added
							dbg(("%s: parsed %d const(s) from %s"):format(mod, added, path))
						else
							dbg(("%s: failed to read %s"):format(mod, path), vim.log.levels.WARN)
						end
					end

					dbg(("%s: total parsed const(s)=%d"):format(mod, total_added))
				end

				load_mod("tailwind", "tailwind.rs")
				load_mod("css", "css.rs")

				local n500 = palettes.tailwind.NEUTRAL_500
				if n500 then
					dbg(("tailwind::NEUTRAL_500=%s a=%s"):format(n500.hex, tostring(n500.a)))
				else
					dbg("tailwind::NEUTRAL_500 not found in parsed palettes", vim.log.levels.WARN)
				end
			end

			local function palette_entry(mod, name)
				load_palettes_once()
				return palettes[mod] and palettes[mod][name] or nil
			end

			local function mk_palette_highlighter(key, mod)
				opts.highlighters[key] = {
					pattern = mod .. "::[%u][%u%d_]*",
					group = function(buf_id, match)
						if not is_rust(buf_id) then
							return nil
						end

						local name = match:match(mod .. "::([%u%d_]+)")
						if not name then
							return nil
						end

						local entry = palette_entry(mod, name)
						if not entry then
							if dbg_enabled() and not miss_logged[match] then
								miss_logged[match] = true
								dbg(("MISS %s (no palette entry)"):format(match), vim.log.levels.WARN)
							end
							return nil
						end

						local hex = entry.hex
						if entry.a and entry.a < 1 then
							hex = blend_hex_over_bg(hex, get_normal_bg_hex(), entry.a)
						end

						return group_from_hex(hex)
					end,
				}
			end

			mk_palette_highlighter("bevy_palette_tailwind", "tailwind")
			mk_palette_highlighter("bevy_palette_css", "css")

			-- Optional: highlight full call Color::Srgba(tailwind::FOO)
			opts.highlighters.bevy_color_Srgba_palette = {
				pattern = "Color::Srgba%s*%b()",
				group = function(buf_id, match)
					if not is_rust(buf_id) then
						return nil
					end

					local inner = match:match("Color::Srgba%s*%(%s*(.-)%s*%)")
					if not inner then
						return nil
					end

					local mod, name = inner:match("^(%a+)::([%u%d_]+)$")
					if mod ~= "tailwind" and mod ~= "css" then
						return nil
					end

					local entry = palette_entry(mod, name)
					if not entry then
						return nil
					end

					local hex = entry.hex
					if entry.a and entry.a < 1 then
						hex = blend_hex_over_bg(hex, get_normal_bg_hex(), entry.a)
					end

					return group_from_hex(hex)
				end,
			}

			-- Debug command (safe to define once)
			if not vim.g._bevy_hipatterns_debug_cmd then
				vim.g._bevy_hipatterns_debug_cmd = true

				vim.api.nvim_create_user_command("BevyHipatternsDebug", function()
					load_palettes_once()
					local n_tailwind = 0
					for _ in pairs(palettes.tailwind) do
						n_tailwind = n_tailwind + 1
					end
					local n_css = 0
					for _ in pairs(palettes.css) do
						n_css = n_css + 1
					end

					local n500 = palettes.tailwind.NEUTRAL_500
					local msg = {
						("cargo_home=%s"):format(cargo_home()),
						("tailwind entries=%d"):format(n_tailwind),
						("css entries=%d"):format(n_css),
						("tailwind::NEUTRAL_500=%s"):format(n500 and (n500.hex .. " a=" .. tostring(n500.a)) or "nil"),
						("tailwind.rs candidates=%d"):format(#find_palette_files("tailwind.rs")),
					}
					vim.notify(table.concat(msg, "\n"), vim.log.levels.INFO)
				end, {})
			end
		end,
	},
}
