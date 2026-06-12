#!/usr/bin/env bash
# Renders og/card.html into static/og.png (1200×630) with headless Chrome,
# then quantizes it so social crawlers get a small file.
set -euo pipefail
cd "$(dirname "$0")/.."

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"

"$CHROME" --headless=new --disable-gpu --hide-scrollbars \
	--force-device-scale-factor=1 --window-size=1200,630 \
	--screenshot=static/og.png --allow-file-access-from-files \
	"file://$PWD/og/card.html"

if command -v pngquant >/dev/null; then
	pngquant --quality=80-95 --force --output static/og.png static/og.png
fi

ls -la static/og.png
