<p align="center">
  <img src="store/icons/io.github.nnfisfree.govee-control.png" width="96" alt="Govee Control icon" />
</p>

<h1 align="center">Govee Control for OpenDeck</h1>

<p align="center">
  Control your Govee lights from an Elgato Stream Deck with
  <a href="https://github.com/nekename/OpenDeck">OpenDeck</a>, the free and open-source Stream Deck app.
</p>

---

## Features

- **Power**: toggle, on or off
- **Brightness**: set a level, step up or down, or turn a dial
- **Color**: pick an RGB colour, or turn a dial through the hues
- **Color Temperature**: set or step a white temperature in Kelvin, or turn a dial
- **Groups**: tick several lights on any key or dial to control them together
- **Stream Deck+ dials**: every action works on a dial. Turn it to adjust, press or tap it to toggle power, and the touch strip shows the current value.
- **Native**: one small binary per platform, with no Node.js, Wine or extra runtime. Builds are available for Linux (x86_64, ARM64), Windows and macOS.

The plugin uses Govee's **official cloud API** with your own free API key.

> This project is not affiliated with, endorsed by or sponsored by Govee.
> "Govee" is a trademark of its owner and is used here only to describe compatibility.

## 1. Get a Govee API key

1. Open the **Govee Home** app on your phone and sign in.
2. Go to **Profile → Settings (⚙) → Apply for API Key**.
3. Fill in your name and a reason (for example "Control my lights from Stream Deck"), accept the terms and submit.
4. Govee emails the key to your account address, usually within a few minutes. Check your spam folder if it doesn't arrive.

Only lights that the Govee API supports (most Wi-Fi lights) show up. Bluetooth-only devices aren't available through the API.
If you request a new key later, Govee immediately invalidates the old one, so paste the new key into the plugin.

## 2. Install

**From the OpenDeck store:** open OpenDeck → *Plugins* → search for "Govee Control" → *Install*.

**Manually:** download `io.github.nnfisfree.govee-control.streamDeckPlugin` from the
[latest release](https://github.com/nnfisfree/govee-control-opendeck/releases/latest),
then in OpenDeck go to *Plugins* → *Install from file*.

## 3. Use it

1. Drag any **Govee Control** action onto a key or dial.
2. In the settings panel, paste your API key and click **Save**. The plugin checks it with Govee and loads your lights. You only need to do this once; every action shares the key.
3. Tick the light or lights that this key or dial should control, then set the action's options.

| Action | Key | Dial (Stream Deck+) |
|---|---|---|
| Power | Toggle, on or off | Press or tap: same as the key |
| Brightness | Set a %, or step by ±N% (shows the current %) | Turn: ±% per tick. Press: toggle power |
| Color | Apply the chosen colour (the key shows it) | Turn: shift the hue. Press: toggle power |
| Color Temperature | Set a Kelvin value, or step by ±N K | Turn: ±K per tick. Press: toggle power |

**Groups:** when several lights are ticked, commands go to all of them at once. The key shows the state of the first ticked light. For colour temperature, the slider is limited to the range that every ticked light supports.

**Sync:** keys update as soon as you press them. They also re-read the real state from Govee every minute and after your computer wakes up, so changes made in the Govee app or with a wall switch show up too.

## Privacy and security

- Your API key is stored in OpenDeck's plugin settings file, in plain text, on your computer (on Linux: `~/.config/opendeck/settings/io.github.nnfisfree.govee-control.sdPlugin.json`). Anyone with access to your user account can read it. You can remove it at any time with **Remove** in the settings panel.
- The key is never written to logs and is never sent back to the settings panel, which only shows its last four characters.
- The plugin only talks to `openapi.api.govee.com`. There is no telemetry.

See [SECURITY.md](SECURITY.md) to report a vulnerability.

## Limits

Govee rate-limits its API. Per device, control is limited to 2 requests per second and state reads to 30 per minute. Per account, control is limited to 12 requests per second. The plugin queues requests to stay within these limits, and it waits until you stop turning a dial (about ¼ second) before sending one update with the final value. If Govee still refuses a request, the key shows a ⚠ alert.

Because this is a cloud API, each command takes a moment to reach the light, usually well under a second.

## Troubleshooting

- **⚠ on a key:** either no API key is set, no light is ticked, or Govee rejected the request. The plugin log has the details. On Linux it is at `~/.local/share/opendeck/logs/plugins/io.github.nnfisfree.govee-control.sdPlugin.log`.
- **A light is missing:** click **Refresh** in the settings panel. Lights added to your Govee account after the plugin started appear after a refresh.
- **"Unavailable device"** in the list means Govee no longer reports that light. It might have been removed from your account. Untick it.

## Project layout

```
govee-control-opendeck/
├── assets/                  Plugin bundle contents (packaged into <bundle-id>.sdPlugin/)
│   ├── manifest.json        OpenDeck plugin manifest (actions, binaries per platform)
│   ├── icons/               Plugin, category, action and key icons (SVG)
│   └── property-inspector/  Settings panels shown in OpenDeck (HTML/CSS/JS)
├── crates/
│   ├── govee-api/           Govee Platform API client, rate limiter, colour helpers (library)
│   ├── opendeck-plugin/     The plugin binary: actions, shared state, key images, end-to-end test
│   └── govee-cli/           Developer tool to try the API with your key (not shipped)
├── docs/                    Architecture, development and release guides
├── scripts/                 Install-for-development, packaging and store-icon scripts
├── store/                   OpenDeck store catalogue entry and icon
└── .github/                 CI, release workflow, issue and PR templates
```

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started, and:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how the plugin works
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): building, testing and running it locally
- [docs/RELEASING.md](docs/RELEASING.md): releases and the OpenDeck store

## Similar plugins

- [Govee Light Controller](https://github.com/DavidGolunski/GoveeLightController) uses Govee's LAN API, so it needs no API key. It is a Windows Stream Deck plugin, so on Linux it runs under Wine.
- [Govee Controller](https://github.com/GeekyEggo/GoveeController) is an Elgato Stream Deck plugin.

This plugin is a native OpenDeck plugin for Linux, Windows and macOS, with dial support and groups.

## License

[MIT](LICENSE) © nnfisfree
