# Changelog

## Unreleased

## 0.2.0 (2026-11-23)

First public release, and the first one in the OpenDeck store.

### New actions
- **Scene**: play a built-in Govee scene, a DIY scene or a saved snapshot. On a dial, turn to browse the scenes.
- **Preset**: set power, colour (or white temperature) and brightness in one press.
- **Cycle**: step through a list of colours, brightness levels, white temperatures or scenes.
- **Music Mode**: start music-reactive mode with a chosen style, sensitivity and colour.
- **Segment Color**: colour or dim individual segments of light strips and bars, with one colour or a gradient.
- **Toggle**: switch device features such as the gradient, night light, DreamView or oscillation.
- **Sensor**: show the temperature and humidity of Govee thermometers and hygrometers.
- **Appliance**: set the work mode or target temperature of Govee heaters, fans, purifiers and humidifiers.

### Improvements
- Long press: every key action can do something else when held for half a second.
- Keys and dials show "Offline" when Govee reports the device as unreachable.
- The Color dial can step through a list of your own colours instead of shifting the hue.
- The Power action also lists Govee appliances.
- Scene and music-mode names are looked up on each device, so groups can mix light models.
- Settings panels list only the devices that support each action, and say "devices" instead of "lights".

### Changed
- New project layout with top-level folders by role: `plugin/`, `api/`, `cli/`, `ui/` and `bundle/`.
- The README is now also the plugin's page in the OpenDeck store, with a supported-devices table.

## 0.1.0

Release candidate, not published. It is included in 0.2.0.

- Actions: Power, Brightness, Color and Color Temperature.
- Every action works on keys and on Stream Deck+ dials (turn to adjust, press or tap to toggle power).
- Groups: select several lights on any key or dial to control them together.
- Uses the official Govee Platform API with your own API key. The plugin validates the key, stores it in OpenDeck's global settings and never shows it again.
- Built-in rate limiting and dial debouncing keep requests within Govee's limits.
- Keys stay in sync with changes made elsewhere: state is re-read every minute and after the computer wakes up.
- Native builds for Linux (x86_64, aarch64), Windows (x86_64) and macOS (Intel, Apple Silicon).
