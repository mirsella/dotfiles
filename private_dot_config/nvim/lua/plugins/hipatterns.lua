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

			-- Bevy Color constructors
			mk_color_u8_highlighter("bevy_color_srgb_u8", "Color::srgb_u8", 3)
			mk_color_u8_highlighter("bevy_color_rgb_u8", "Color::rgb_u8", 3)
			mk_color_u8_highlighter("bevy_color_srgba_u8", "Color::srgba_u8", 4)
			mk_color_u8_highlighter("bevy_color_rgba_u8", "Color::rgba_u8", 4)

			mk_color_float_highlighter("bevy_color_srgb", "Color::srgb", 3)
			mk_color_float_highlighter("bevy_color_rgb", "Color::rgb", 3)
			mk_color_float_highlighter("bevy_color_srgba", "Color::srgba", 4)
			mk_color_float_highlighter("bevy_color_rgba", "Color::rgba", 4)

			-- Color::hex("RRGGBB") / "#RRGGBB" / "RRGGBBAA"
			opts.highlighters.bevy_color_hex = {
				pattern = 'Color::hex%s*%(%s*".-"%s*%)',
				group = function(buf_id, match)
					if not is_rust(buf_id) then
						return nil
					end

					local s = match:match('Color::hex%s*%(%s*"(.-)"%s*%)')
					if not s then
						return nil
					end

					local h = s:gsub("^#", ""):upper()
					if (#h ~= 6 and #h ~= 8) or not h:match("^%x+$") then
						return nil
					end

					local hex = "#" .. h:sub(1, 6)
					if #h == 8 then
						local a_u8 = tonumber(h:sub(7, 8), 16)
						if not a_u8 then
							return nil
						end
						local a = clamp(a_u8 / 255, 0, 1)
						if a < 1 then
							hex = blend_hex_over_bg(hex, get_normal_bg_hex(), a)
						end
					end

					return group_from_hex(hex)
				end,
			}

			-- Re-enable Color::WHITE, Color::BLACK, etc.
			local named = {
				WHITE = "#FFFFFF",
				BLACK = "#000000",
				RED = "#FF0000",
				GREEN = "#00FF00",
				BLUE = "#0000FF",
				YELLOW = "#FFFF00",
				CYAN = "#00FFFF",
				MAGENTA = "#FF00FF",
				GRAY = "#808080",
				GREY = "#808080",
			}

			opts.highlighters.bevy_color_named = {
				pattern = "Color::%u[%u%d_]*",
				group = function(buf_id, match)
					if not is_rust(buf_id) then
						return nil
					end

					local name = match:match("Color::(%u[%u%d_]*)")
					local hex = name and named[name] or nil
					if not hex then
						return nil
					end

					return group_from_hex(hex)
				end,
			}

			-- Load hardcoded palettes (no FS searching at runtime).
			local palettes = nil
			do
				local ok, mod = pcall(require, "bevy_palettes")
				if ok and type(mod) == "table" then
					palettes = mod
				end
			end

			local function palette_lookup(mod, name)
				if not palettes then
					return nil
				end
				local t = palettes[mod]
				if type(t) ~= "table" then
					return nil
				end

				local v = t[name]
				if type(v) == "string" then
					return { hex = v, a = 1 }
				end
				if type(v) == "table" and type(v.hex) == "string" then
					return { hex = v.hex, a = v.a or 1 }
				end
				return nil
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

						local entry = palette_lookup(mod, name)
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
			end

			mk_palette_highlighter("bevy_palette_tailwind", "tailwind")
			mk_palette_highlighter("bevy_palette_css", "css")

			-- Optional: highlight full call Color::Srgba(tailwind::FOO) as well
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

					local entry = palette_lookup(mod, name)
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

			-- One-time generator: create lua/bevy_palettes.lua from local bevy_color sources.
			if not vim.g._bevy_palettes_generate_cmd then
				vim.g._bevy_palettes_generate_cmd = true

				local function parse_float(s)
					local n = s:match("[%+%-]?%d*%.?%d+")
					return n and tonumber(n) or nil
				end

				local function round_u8_from_float01(x)
					return clamp_u8(math.floor(clamp(x, 0, 1) * 255 + 0.5))
				end

				local function extract_srgba_consts(content)
					local out = {}

					-- Srgba::rgb(r,g,b)
					for name, r, g, b in
						content:gmatch(
							"pub%s+const%s+([%u%d_]+)%s*:%s*Srgba%s*=%s*Srgba::rgb%s*%(%s*([^,]+)%s*,%s*([^,]+)%s*,%s*([^)]+)%s*%)%s*;"
						)
					do
						local rr = parse_float(r)
						local gg = parse_float(g)
						local bb = parse_float(b)
						if rr and gg and bb then
							out[name] = rgb_u8_to_hex(
								round_u8_from_float01(rr),
								round_u8_from_float01(gg),
								round_u8_from_float01(bb)
							)
						end
					end

					-- Srgba::new(r,g,b,a) (ignore alpha if 1; keep if not)
					for name, r, g, b, a in
						content:gmatch(
							"pub%s+const%s+([%u%d_]+)%s*:%s*Srgba%s*=%s*Srgba::new%s*%(%s*([^,]+)%s*,%s*([^,]+)%s*,%s*([^,]+)%s*,%s*([^)]+)%s*%)%s*;"
						)
					do
						local rr = parse_float(r)
						local gg = parse_float(g)
						local bb = parse_float(b)
						local aa = parse_float(a)
						if rr and gg and bb and aa then
							local hex = rgb_u8_to_hex(
								round_u8_from_float01(rr),
								round_u8_from_float01(gg),
								round_u8_from_float01(bb)
							)
							if aa < 1 then
								out[name] = { hex = hex, a = clamp(aa, 0, 1) }
							else
								out[name] = hex
							end
						end
					end

					return out
				end

				local function table_keys_sorted(t)
					local ks = {}
					for k in pairs(t) do
						ks[#ks + 1] = k
					end
					table.sort(ks)
					return ks
				end

				local function lua_quote(s)
					return string.format("%q", s)
				end

				vim.api.nvim_create_user_command("BevyPalettesGenerate", function(cmd)
					local args = {}
					for token in (cmd.args or ""):gmatch("%S+") do
						local k, v = token:match("^(%w+)%=(.+)$")
						if k and v then
							args[k] = vim.fn.expand(v)
						end
					end

					local tailwind_path = args.tailwind
					local css_path = args.css

					if not tailwind_path or tailwind_path == "" then
						vim.notify("BevyPalettesGenerate: missing tailwind=PATH", vim.log.levels.ERROR)
						return
					end

					if not css_path or css_path == "" then
						vim.notify("BevyPalettesGenerate: missing css=PATH", vim.log.levels.ERROR)
						return
					end

					local out_path = args.out or (vim.fn.stdpath("config") .. "/lua/bevy_palettes.lua")

					local function read(path)
						local ok, lines = pcall(vim.fn.readfile, path)
						if not ok or not lines then
							return nil
						end
						return table.concat(lines, "\n")
					end

					local tw = read(tailwind_path)
					if not tw then
						vim.notify("BevyPalettesGenerate: failed to read " .. tailwind_path, vim.log.levels.ERROR)
						return
					end

					local css = read(css_path)
					if not css then
						vim.notify("BevyPalettesGenerate: failed to read " .. css_path, vim.log.levels.ERROR)
						return
					end

					local tailwind_tbl = extract_srgba_consts(tw)
					local css_tbl = extract_srgba_consts(css)

					local function count_entries(t)
						local n = 0
						for _ in pairs(t) do
							n = n + 1
						end
						return n
					end

					local dir = vim.fn.fnamemodify(out_path, ":h")
					vim.fn.mkdir(dir, "p")

					local lines = {}
					lines[#lines + 1] = "-- Auto-generated by :BevyPalettesGenerate"
					lines[#lines + 1] = "return {"
					lines[#lines + 1] = "  tailwind = {"
					for _, k in ipairs(table_keys_sorted(tailwind_tbl)) do
						local v = tailwind_tbl[k]
						if type(v) == "string" then
							lines[#lines + 1] = string.format("    %s = %s,", k, lua_quote(v))
						else
							lines[#lines + 1] =
								string.format("    %s = { hex = %s, a = %s },", k, lua_quote(v.hex), tostring(v.a))
						end
					end
					lines[#lines + 1] = "  },"
					lines[#lines + 1] = "  css = {"
					for _, k in ipairs(table_keys_sorted(css_tbl)) do
						local v = css_tbl[k]
						if type(v) == "string" then
							lines[#lines + 1] = string.format("    %s = %s,", k, lua_quote(v))
						else
							lines[#lines + 1] =
								string.format("    %s = { hex = %s, a = %s },", k, lua_quote(v.hex), tostring(v.a))
						end
					end
					lines[#lines + 1] = "  },"
					lines[#lines + 1] = "}"

					local ok = pcall(vim.fn.writefile, lines, out_path)
					if not ok then
						vim.notify("BevyPalettesGenerate: failed to write " .. out_path, vim.log.levels.ERROR)
						return
					end

					vim.notify(
						("Wrote %s (tailwind=%d, css=%d). Restart Neovim."):format(
							out_path,
							count_entries(tailwind_tbl),
							count_entries(css_tbl)
						),
						vim.log.levels.INFO
					)
				end, {
					nargs = "*",
				})
			end
		end,
	},
}
