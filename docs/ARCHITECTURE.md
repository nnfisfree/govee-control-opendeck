# Architecture

```
Stream Deck ──USB── OpenDeck ──WebSocket── govee-control-opendeck ──HTTPS── openapi.api.govee.com ── lights
                       │                         (this plugin)
                       └── settings panel ──WebSocket──┘
                           (ui/)
```

## How OpenDeck runs the plugin

1. OpenDeck reads `manifest.json` (from `bundle/`) in the installed `<bundle-id>.sdPlugin/` folder. It picks the binary for the current platform from `CodePaths`, which is keyed by Rust target triple.
2. It starts the binary with `-port <n> -pluginUUID <uuid> -registerEvent registerPlugin -info <json>`.
3. The plugin connects back over WebSocket and speaks the [OpenAction protocol](https://openaction.amankhanna.me/), which is compatible with Elgato's Stream Deck SDK v2. The [`openaction`](https://crates.io/crates/openaction) crate handles the protocol.
4. Anything the plugin writes to stdout or stderr ends up in OpenDeck's log folder as `plugins/<uuid>.log`.

The `openaction` crate processes events one at a time. Handlers therefore return quickly and do any network work in spawned tokio tasks.

## Crates

### `api/` (library crate `govee-api`)

| File | Purpose |
|---|---|
| `client.rs` | `GoveeClient`: `devices()`, `state()`, `control()`, `scenes()`. Sends the `Govee-API-Key` header, maps errors (401 → `Unauthorized`, 429 → one retry then `RateLimited`, error `code` in the body → `Api`), and never includes the key in errors or `Debug` output. `Command` maps power, brightness, colour, Kelvin, scenes, music mode, segments, toggles, work modes and target temperature to Govee capabilities. |
| `types.rs` | Serde models for devices, capabilities and state. `Device::features()` works out what a device supports (Kelvin range, segment count, scenes, toggles, sensors, appliance modes). |
| `ratelimit.rs` | Token buckets matching Govee's documented limits: per device, per account, and for state reads, scene lists and the device list. |
| `color.rs` | Packed RGB, hex, hue and Kelvin → display colour conversions. |

Debug builds read `GOVEE_API_BASE_URL` so that tests can point the client at a mock server. Release builds always use the real API.

### `plugin/` (binary `govee-control-opendeck`)

| File | Purpose |
|---|---|
| `main.rs` | Logger, global events (loading the API key from global settings, wake-up resync) and the 60-second state poller. |
| `state.rs` | Shared state: the client, device list, cached device states and scene lists, tracked action instances, debounce generations and per-instance dial positions. `send()` takes per-device command lists (`Jobs`), sent in order for each device. |
| `actions/mod.rs` | Shared lifecycle (`appeared`, `disappeared`), `dispatch()` / `dispatch_jobs()` (optimistic update, dial debounce, concurrent send, alert and re-sync on failure), long press (`key_down` / `key_up` with `Hold` settings) and the offline indicator. |
| `actions/*.rs` | One file per action (power, brightness, color, temperature, scene, preset, cycle, music, segment, toggle, sensor, appliance): its settings, key and dial handlers, and rendering (`setState`, `setTitle`, `setImage` or `setFeedback`). |
| `pi.rs` | Messages between the settings panel and the plugin (`init`, `setApiKey`, `clearApiKey`, `refresh`, `scenes`, and the `status` and `scenes` replies). |
| `render.rs` | Generated SVG key images (bulbs, segment strips, sensor readings, offline) and dial icons. |
| `tests/e2e.rs` | Runs the real binary against a fake OpenDeck WebSocket server and a mock Govee API. |

### `cli/` (crate `govee-cli`)

A small command-line tool for trying the API with your own key. It isn't packaged into the plugin.

## Settings panel (property inspector)

`ui/common.js` is shared by every action page. It handles:
- the API key section
- the device picker, which lists only devices that support the action
- the long-press section added to every key action
- helpers for pages that fill lists from device capabilities (`fillSelect`, `selectedDevices`, `save`)
- binding inputs to settings through `data-setting`, `data-show-if` and `data-controller` attributes

The panel never stores the API key. It sends the key to the plugin, which checks it against Govee, saves it with `setGlobalSettings`, and replies with only a masked hint.

## Data flow for a key press

1. OpenDeck sends `keyDown` with the instance settings, which include the target devices.
2. The action computes the command, for example a brightness step from the cached state of the first device.
3. `dispatch()` updates the cache optimistically and redraws every visible key, so feedback is instant.
4. A background task sends the command to each device through the rate limiter.
5. On a total failure the key shows an alert and the real state is fetched again. Every visible key is then redrawn.

Dial turns go through the same path. Each turn starts a new debounce generation, and only the last one, 250 ms after the dial stops, is actually sent (600 ms for scenes).

Scenes and music modes have different values on different light models. Their settings store the *name*, and `dispatch_later()` looks the value up for each device (fetching and caching scene lists as needed) before sending.

With a long-press action set, `key_down` starts a 500 ms timer instead of acting. If the key is still down when it fires, the long-press commands are sent; otherwise `key_up` runs the normal press.
