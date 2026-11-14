# Security policy

## Reporting a vulnerability

Please **don't** open a public issue for security problems. Instead, use GitHub's
[private vulnerability reporting](https://github.com/nnfisfree/govee-control-opendeck/security/advisories/new).
You'll get a reply as soon as possible.

## Your Govee API key

- The plugin stores your key in OpenDeck's settings file on your computer, in plain text, readable by your user account. It isn't stored anywhere else.
- The key is sent only to `openapi.api.govee.com`, over HTTPS. It is never logged and never shown again in the settings panel.
- If you think your key has leaked, request a new one in the Govee Home app (Profile → Settings → Apply for API Key). This immediately invalidates the old key. Then paste the new key into the plugin.
- Never include your key in issues, pull requests, screenshots or logs.
