# Shared helpers for the scripts in this folder. Source it, don't run it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The bundle ID is the prefix of every action UUID in bundle/manifest.json.
BUNDLE_ID="$(python3 -c 'import json,sys; a=json.load(open(sys.argv[1]))["Actions"][0]["UUID"]; print(a.rsplit(".",1)[0])' "$ROOT/bundle/manifest.json")"
PLUGIN_DIR_NAME="$BUNDLE_ID.sdPlugin"
BIN_NAME="govee-control-opendeck"

host_triple() {
	rustc -vV | sed -n 's/^host: //p'
}

# Copy the manifest, icons and settings panels into $1 (an .sdPlugin folder).
# Inside the bundle the panels live in property-inspector/, as manifest.json expects.
copy_assets() {
	local dest="$1"
	mkdir -p "$dest/bin"
	cp "$ROOT/bundle/manifest.json" "$dest/"
	cp -r "$ROOT/bundle/icons" "$dest/icons"
	cp -r "$ROOT/ui" "$dest/property-inspector"
	cp "$ROOT/LICENSE" "$dest/" 2>/dev/null || true
}
