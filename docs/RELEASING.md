# Releasing and publishing

## Cut a release

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and add a section to `CHANGELOG.md`.
2. Commit, then tag and push:
   ```sh
   git tag v0.1.0
   git push origin main v0.1.0
   ```
3. The **Release** workflow (`.github/workflows/release.yml`):
   - checks that the tag matches the `Cargo.toml` version
   - builds `govee-control-opendeck` for five targets:
     - `x86_64-unknown-linux-gnu`
     - `aarch64-unknown-linux-gnu`
     - `x86_64-pc-windows-msvc`
     - `x86_64-apple-darwin`
     - `aarch64-apple-darwin`
   - runs `scripts/package.sh`, which produces `io.github.nnfisfree.govee-control.streamDeckPlugin` (a zip containing `io.github.nnfisfree.govee-control.sdPlugin/`)
   - publishes a GitHub release with that file as its only asset. When a release has a single plugin file, the OpenDeck store installs it without asking.

To package locally from binaries you already have:

```sh
scripts/package.sh <dir-with-govee-control-opendeck-<triple>-binaries> 0.1.0
```

## Publish to the OpenDeck store (first time)

The store catalogue is [OpenActionAPI/plugins](https://github.com/OpenActionAPI/plugins). Its automated check reads the repository through the public GitHub API, and OpenDeck downloads releases anonymously. The repository must therefore be **public** before you submit.

1. On GitHub, in the repository settings:
   - Set the **description** to exactly the `description` value in `store/catalogue-entry.json`.
   - Add the topic **`openaction`**.
2. Publish a release (see above).
3. Fork `OpenActionAPI/plugins`, then:
   - Add the entry from `store/catalogue-entry.json` to `catalogue.json`, in the **native plugins** section. Within that section, entries are sorted by repository URL, case-insensitively.
   - Keep `catalogue.json` exactly as `json.dumps(..., indent="\t")` writes it: tabs, no trailing spaces, and a final newline.
   - Copy `store/icons/io.github.nnfisfree.govee-control.png` to `icons/` in the fork. To regenerate the icon, run `scripts/store-icon.sh`.
4. Open a pull request. The check verifies:
   - the topic
   - that the manifest `Name` and `Author` match the entry
   - the description
   - the bundle ID
   - that the latest release has a plugin file
   - the icon size
   - the entry's ordering

Later releases need no store changes. OpenDeck always installs the latest GitHub release.

## Never change the bundle ID

The bundle ID is `io.github.nnfisfree.govee-control`. It is the folder name users have installed, and it is part of every action UUID saved in their profiles. Changing it breaks existing installs.
