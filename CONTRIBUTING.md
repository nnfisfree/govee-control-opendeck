# Contributing

Thanks for helping improve Govee Control for OpenDeck!

## Reporting bugs

Open an issue with the **Bug report** template. Please include:

- your OS, OpenDeck version and plugin version
- your Stream Deck model, and the Govee light model (SKU, for example `H6008`)
- the relevant lines from the plugin log (`~/.local/share/opendeck/logs/plugins/io.github.nnfisfree.govee-control.sdPlugin.log` on Linux)

**Never post your Govee API key.** The plugin doesn't write it to the log, but check anything you paste.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Set up your environment as described in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the code is organised.
3. Make sure these pass before opening the PR:
   ```sh
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
4. Add tests for new behaviour where practical. The Govee client is tested against a mock server, and the plugin with `crates/opendeck-plugin/tests/e2e.rs`.
5. Add a line to the `Unreleased` section of `CHANGELOG.md`.

## Ideas on the roadmap

- Scenes (dynamic and DIY scenes)
- Segment colours for light strips
- An optional LAN-API mode for supported lights

If you plan to work on one of these, open an issue first so we can agree on the approach.
