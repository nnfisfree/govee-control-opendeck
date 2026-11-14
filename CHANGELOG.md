# Changelog

## Unreleased

## 0.1.0

First release.

- Actions: Power, Brightness, Color and Color Temperature.
- Every action works on keys and on Stream Deck+ dials (turn to adjust, press or tap to toggle power).
- Groups: select several lights on any key or dial to control them together.
- Uses the official Govee Platform API with your own API key. The plugin validates the key, stores it in OpenDeck's global settings and never shows it again.
- Built-in rate limiting and dial debouncing keep requests within Govee's limits.
- Keys stay in sync with changes made elsewhere: state is re-read every minute and after the computer wakes up.
- Native builds for Linux (x86_64, aarch64), Windows (x86_64) and macOS (Intel, Apple Silicon).
