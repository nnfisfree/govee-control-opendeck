#!/usr/bin/env bash
# Assemble the distributable plugin from prebuilt binaries.
#
#   scripts/package.sh <dir-with-binaries> [version]
#
# The directory must contain binaries named govee-control-opendeck-<target-triple>[.exe],
# as produced by the release workflow. The result is
# dist/<bundle-id>.streamDeckPlugin, a zip with <bundle-id>.sdPlugin/ at its root,
# which OpenDeck installs directly.
source "$(dirname "$0")/common.sh"

BINARIES="${1:?usage: package.sh <dir-with-binaries> [version]}"
VERSION="${2:-}"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PLUGIN="$STAGE/$PLUGIN_DIR_NAME"
copy_assets "$PLUGIN"

if [[ -n "$VERSION" ]]; then
	python3 - "$PLUGIN/manifest.json" "$VERSION" <<'PY'
import json, sys
path, version = sys.argv[1], sys.argv[2].lstrip("v")
with open(path) as f:
    manifest = json.load(f)
manifest["Version"] = version
with open(path, "w") as f:
    json.dump(manifest, f, indent="\t", ensure_ascii=False)
    f.write("\n")
PY
fi

# Every binary named in CodePaths must be present.
missing=0
while read -r rel; do
	src="$BINARIES/$(basename "$rel")"
	if [[ -f "$src" ]]; then
		install -m 755 "$src" "$PLUGIN/$rel"
	else
		echo "missing binary: $(basename "$rel")" >&2
		missing=1
	fi
done < <(python3 -c 'import json,sys; [print(p) for p in json.load(open(sys.argv[1]))["CodePaths"].values()]' "$PLUGIN/manifest.json")
[[ $missing == 0 ]] || exit 1

mkdir -p "$ROOT/dist"
OUT="$ROOT/dist/$BUNDLE_ID.streamDeckPlugin"
rm -f "$OUT"
# Python's zipfile keeps Unix permissions (the binaries must stay executable)
# and needs no extra tools.
python3 - "$STAGE" "$PLUGIN_DIR_NAME" "$OUT" <<'PY'
import os, sys, zipfile
stage, folder, out = sys.argv[1:]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for dirpath, dirnames, filenames in os.walk(os.path.join(stage, folder)):
        dirnames.sort()
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            arc = os.path.relpath(full, stage)
            info = zipfile.ZipInfo.from_file(full, arc)
            info.compress_type = zipfile.ZIP_DEFLATED
            with open(full, "rb") as f:
                z.writestr(info, f.read())
PY
echo "Created $OUT"
