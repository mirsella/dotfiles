#!/usr/bin/env bash
# Open image(s) in tldraw web from Dolphin.
# tldraw.com cannot load local file:// paths directly, so this copies the
# first image to the clipboard (Wayland) and opens tldraw — paste with Ctrl+V.
set -euo pipefail

if [[ $# -eq 0 ]]; then
  xdg-open "https://www.tldraw.com/" &>/dev/null &
  exit 0
fi

file="$1"
# Normalize file:// URLs to plain paths (in case %U is used)
file="${file#file://}"
if [[ "$file" == *%* ]]; then
  printf -v file '%b' "${file//%/\\x}"
fi

mime="$(file --mime-type -b "$file")"
case "$mime" in
  image/png|image/jpeg|image/webp) wl-copy --type "$mime" < "$file" ;;
  *) wl-copy < "$file" ;;
esac

xdg-open "https://www.tldraw.com/" &>/dev/null &

if [[ $# -gt 1 ]]; then
  notify-send "tldraw opened" "First of $# images copied. Press Ctrl+V in tldraw, then repeat for the rest."
else
  notify-send "tldraw opened" "Image copied to clipboard — press Ctrl+V in tldraw."
fi
