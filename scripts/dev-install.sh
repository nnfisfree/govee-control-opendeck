#!/usr/bin/env bash
# Build the plugin for this machine and install it into OpenDeck's plugin folder.
# Restart OpenDeck (or reload the plugin from its settings) afterwards.
#
#   scripts/dev-install.sh            # release build
#   scripts/dev-install.sh --debug    # debug build (more logging)
source "$(dirname "$0")/common.sh"

PROFILE=release
CARGO_FLAGS=(--release)
if [[ "${1:-}" == "--debug" ]]; then
	PROFILE=debug
	CARGO_FLAGS=()
fi

if [[ -n "${OPENDECK_CONFIG:-}" ]]; then
	CONFIG="$OPENDECK_CONFIG"
elif [[ -d "$HOME/.var/app/me.amankhanna.opendeck/config/opendeck" ]]; then
	CONFIG="$HOME/.var/app/me.amankhanna.opendeck/config/opendeck" # Flatpak
else
	CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/opendeck"
fi

TRIPLE="$(host_triple)"
(cd "$ROOT" && cargo build "${CARGO_FLAGS[@]}" --bin "$BIN_NAME")

DEST="$CONFIG/plugins/$PLUGIN_DIR_NAME"
rm -rf "$DEST"
copy_assets "$DEST"
install -m 755 "$ROOT/target/$PROFILE/$BIN_NAME" "$DEST/bin/$BIN_NAME-$TRIPLE"

echo "Installed $PLUGIN_DIR_NAME ($PROFILE, $TRIPLE) to $DEST"
echo "Restart OpenDeck to load it. Plugin log: OpenDeck log folder → plugins/$PLUGIN_DIR_NAME.log"
