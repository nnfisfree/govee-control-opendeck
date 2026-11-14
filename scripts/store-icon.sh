#!/usr/bin/env bash
# Render the PNG icon required by the OpenAction plugin catalogue
# (icons/<bundle-id>.png, at least 144×144) from assets/icons/plugin.svg.
# The result is committed in store/icons/ so it is ready for the store PR.
source "$(dirname "$0")/common.sh"
OUT="$ROOT/store/icons/$BUNDLE_ID.png"
mkdir -p "$(dirname "$OUT")"
rsvg-convert -w 288 -h 288 "$ROOT/assets/icons/plugin.svg" -o "$OUT"
echo "Created $OUT"
