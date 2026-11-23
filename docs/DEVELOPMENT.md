# Development

## Requirements

- Rust (stable), via [rustup](https://rustup.rs) or your distribution
- [OpenDeck](https://github.com/nekename/OpenDeck), to run the plugin
- `rsvg-convert` (librsvg), only needed to regenerate the store icon

## Build and test

```sh
cargo build --workspace
cargo test --workspace                     # unit tests + end-to-end test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt
```

The end-to-end test (`plugin/tests/e2e.rs`) starts the real plugin binary against a fake OpenDeck and a mock Govee API. It needs no network access and no API key.

## Run it in OpenDeck

```sh
scripts/dev-install.sh            # release build, installed into OpenDeck's plugins folder
scripts/dev-install.sh --debug    # debug build with more logging
```

Restart OpenDeck afterwards. The script detects Flatpak installs. Set `OPENDECK_CONFIG` to override the location.

The plugin log is at `~/.local/share/opendeck/logs/plugins/io.github.nnfisfree.govee-control.sdPlugin.log` on Linux.

## Try the Govee API directly

```sh
read -rs GOVEE_API_KEY && export GOVEE_API_KEY   # paste your key; it won't be echoed or saved in history
cargo run -p govee-cli -- devices
cargo run -p govee-cli -- state "Desk lamp"
cargo run -p govee-cli -- power "Desk lamp" on
cargo run -p govee-cli -- brightness "Desk lamp" 40
cargo run -p govee-cli -- color "Desk lamp" "#ff8800"
cargo run -p govee-cli -- kelvin "Desk lamp" 4000
```

## Keep your API key out of the repository

Your Govee API key is personal. Never commit it, paste it into issues, or include it in logs you share.

- The plugin keeps the key in OpenDeck's settings folder, outside this repository.
- For the CLI, use an environment variable as shown above. If you prefer a file, put it in `.env`; `.gitignore` excludes `.env`, `*.key` and `secrets/`.
- Tests use fake keys and a mock server.

## Code style

- `rustfmt.toml` sets tabs and a line width of 120. Run `cargo fmt` before committing.
- CI fails on any clippy warning.
- Keep `api/` free of OpenDeck-specific code.
