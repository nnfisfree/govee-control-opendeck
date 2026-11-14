# Shared helpers for the scripts in this folder. Source it, don't run it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The bundle ID is the prefix of every action UUID in assets/manifest.json.
BUNDLE_ID="$(python3 -c 'import json,sys; a=json.load(open(sys.argv[1]))["Actions"][0]["UUID"]; print(a.rsplit(".",1)[0])' "$ROOT/assets/manifest.json")"
PLUGIN_DIR_NAME="$BUNDLE_ID.sdPlugin"
BIN_NAME="govee-control-opendeck"

host_triple() {
	rustc -vV | sed -n 's/^host: //p'
}

# Copy manifest, icons and property inspector into $1 (an .sdPlugin folder).
copy_assets() {
	local dest="$1"
	mkdir -p "$dest/bin"
	cp "$ROOT/assets/manifest.json" "$dest/"
	cp -r "$ROOT/assets/icons" "$ROOT/assets/property-inspector" "$dest/"
	cp "$ROOT/LICENSE" "$dest/" 2>/dev/null || true
}
