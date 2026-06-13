#!/usr/bin/env bash
# Renders each og/card*.html into static/<name>.png (1200×630) with headless
# Chrome, then quantizes so social crawlers get a small file.
#   card.html               -> static/og.png               (Precedent)
#   card-unprecedented.html -> static/og-unprecedented.png (Unprecedented)
set -euo pipefail
cd "$(dirname "$0")/.."

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"

render() {
	local card="$1" out="$2"
	"$CHROME" --headless=new --disable-gpu --hide-scrollbars \
		--force-device-scale-factor=1 --window-size=1200,630 \
		--screenshot="$out" --allow-file-access-from-files \
		"file://$PWD/og/$card"
	if command -v pngquant >/dev/null; then
		pngquant --quality=80-95 --force --output "$out" "$out"
	fi
	ls -la "$out"
}

render card.html static/og.png
render card-unprecedented.html static/og-unprecedented.png
