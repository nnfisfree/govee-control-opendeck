// Shared Property Inspector logic for the Govee Control plugin.
//
// Each action page includes this script and calls:
//   GoveePI.start({ feature: "power" | "brightness" | "rgb" | "kelvin", defaults: { ... } });
//
// Elements with `data-setting="name"` are bound to the action's settings:
//   - number inputs and range sliders → integers
//   - checkboxes → booleans
//   - everything else → strings
// `data-show-if="setting:value"` shows an element only when that setting has that value.
// `data-controller="Keypad"` or `"Encoder"` shows an element only for that controller.
// `<output data-for="name">` mirrors a setting's value, with an optional `data-suffix`.
//
// The API key is never stored by the PI. It is sent to the plugin, which checks
// it with Govee, saves it in OpenDeck's global settings and only sends back a
// masked hint.

"use strict";

const GoveePI = (() => {
	const API_KEY_HELP_URL = "https://developer.govee.com/reference/apply-you-govee-api-key";

	let socket = null;
	let context = null;
	let controller = "Keypad";
	let settings = {};
	let config = { feature: "power", defaults: {} };
	let status = null;
	let pendingConnection = null;

	const $ = (sel) => document.querySelector(sel);
	const $$ = (sel) => Array.from(document.querySelectorAll(sel));

	function send(event, payload) {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		const message = { event, context };
		if (payload !== undefined) message.payload = payload;
		socket.send(JSON.stringify(message));
	}

	function toPlugin(payload) {
		send("sendToPlugin", payload);
	}

	function saveSettings() {
		send("setSettings", settings);
	}

	function el(tag, attrs = {}, ...children) {
		const node = document.createElement(tag);
		for (const [key, value] of Object.entries(attrs)) {
			if (key === "class") node.className = value;
			else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
			else if (value !== false && value !== undefined && value !== null) node.setAttribute(key, value);
		}
		for (const child of children) {
			if (child === null || child === undefined) continue;
			node.append(child instanceof Node ? child : document.createTextNode(String(child)));
		}
		return node;
	}

	// ------------------------------------------------------------ account ---

	function renderAccount() {
		const box = $("#account");
		box.replaceChildren();
		box.append(el("h2", {}, "Govee account"));

		if (!status) {
			box.append(el("p", { class: "hint" }, "Connecting to plugin…"));
			return;
		}

		if (status.hasKey && !box.dataset.editing) {
			box.append(
				el(
					"div",
					{ class: "row" },
					el("label", {}, "API key"),
					el(
						"div",
						{ class: "inline" },
						el("span", { class: "ok" }, "✓ " + (status.keyHint || "saved")),
						el("button", { onclick: () => ((box.dataset.editing = "1"), renderAccount()) }, "Change"),
						el("button", { onclick: removeKey }, "Remove"),
					),
				),
			);
		} else {
			const input = el("input", {
				type: "password",
				id: "api-key",
				placeholder: "Paste your Govee API key",
				autocomplete: "off",
				spellcheck: "false",
			});
			const save = el("button", { class: "primary", onclick: () => saveKey(input, save) }, "Save");
			input.addEventListener("keydown", (e) => e.key === "Enter" && saveKey(input, save));
			box.append(
				el("div", { class: "row" }, el("label", { for: "api-key" }, "API key"), el("div", { class: "inline" }, input, save)),
				el(
					"p",
					{ class: "hint" },
					"Get a free key in the Govee Home app: Profile → Settings → Apply for API Key. Govee emails it to you. ",
					el("a", { onclick: () => send("openUrl", { url: API_KEY_HELP_URL }) }, "More info"),
				),
			);
			if (status.hasKey) {
				box.append(el("button", { onclick: () => (delete box.dataset.editing, renderAccount()) }, "Cancel"));
			}
		}

		if (status.error) box.append(el("p", { class: "error" }, status.error));
	}

	function saveKey(input, button) {
		const key = input.value.trim();
		if (!key) return;
		button.disabled = true;
		button.textContent = "Checking…";
		delete $("#account").dataset.editing;
		toPlugin({ type: "setApiKey", apiKey: key });
		input.value = "";
	}

	function removeKey() {
		if (!confirm("Remove the Govee API key from OpenDeck? All Govee keys will stop working until you add it again.")) {
			return;
		}
		toPlugin({ type: "clearApiKey" });
	}

	// ------------------------------------------------------------ devices ---

	function sameDevice(a, b) {
		return a.device === b.device && a.sku === b.sku;
	}

	function renderDevices() {
		const box = $("#devices");
		box.replaceChildren();
		box.append(
			el(
				"h2",
				{ class: "inline" },
				"Lights",
				el(
					"button",
					{ style: "margin-left:auto", disabled: !status?.hasKey, onclick: () => toPlugin({ type: "refresh" }) },
					"Refresh",
				),
			),
		);

		if (!status?.hasKey) {
			box.append(el("p", { class: "hint" }, "Add your API key above to load your lights."));
			return;
		}

		const selected = settings.devices || [];
		const compatible = (status.devices || []).filter((d) => d.features[config.feature]);
		// Keep showing selected devices that Govee no longer lists, so they can be unticked.
		const missing = selected.filter((s) => !compatible.some((d) => sameDevice(d, s)));

		if (compatible.length === 0 && missing.length === 0) {
			box.append(el("p", { class: "hint" }, "No lights on this account support this action."));
			return;
		}

		const list = el("div", { class: "devices" });
		for (const device of compatible) list.append(deviceRow(device, device.name, device.sku, false));
		for (const device of missing) list.append(deviceRow(device, "Unavailable device", device.sku, true));
		box.append(list);
		box.append(
			el(
				"p",
				{ class: "hint" },
				"Tick several lights to control them as a group. The first ticked light's state is shown on the key.",
			),
		);
	}

	function deviceRow(device, name, sku, missing) {
		const checked = (settings.devices || []).some((s) => sameDevice(s, device));
		const box = el("input", { type: "checkbox", checked: checked ? "checked" : false });
		box.addEventListener("change", () => {
			const current = (settings.devices || []).filter((s) => !sameDevice(s, device));
			if (box.checked) current.push({ sku: device.sku, device: device.device });
			settings.devices = current;
			saveSettings();
			onSettingsChanged();
		});
		return el(
			"label",
			{ class: "device" + (missing ? " missing" : ""), title: device.device },
			box,
			el("span", { class: "name" }, name),
			el("span", { class: "sku" }, sku),
		);
	}

	// ----------------------------------------------------------- settings ---

	function readInput(input) {
		if (input.type === "checkbox") return input.checked;
		if (input.type === "number" || input.type === "range") {
			const n = parseInt(input.value, 10);
			return Number.isNaN(n) ? undefined : n;
		}
		return input.value;
	}

	function writeInput(input, value) {
		if (value === undefined) return;
		if (input.type === "checkbox") input.checked = !!value;
		else input.value = value;
	}

	function bindSettings() {
		for (const input of $$("[data-setting]")) {
			const name = input.dataset.setting;
			writeInput(input, settings[name]);
			const handler = () => {
				const value = readInput(input);
				if (value === undefined) return;
				settings[name] = value;
				onSettingsChanged();
				saveSettings();
			};
			input.addEventListener(input.type === "range" ? "input" : "change", handler);
		}
	}

	function onSettingsChanged() {
		for (const node of $$("[data-show-if]")) {
			const [name, value] = node.dataset.showIf.split(":");
			node.hidden = String(settings[name]) !== value;
		}
		for (const out of $$("output[data-for]")) {
			const value = settings[out.dataset.for];
			out.textContent = value === undefined ? "" : value + (out.dataset.suffix || "");
		}
		if (typeof config.onChange === "function") config.onChange(settings, status);
	}

	function applyController() {
		for (const node of $$("[data-controller]")) {
			node.hidden = node.dataset.controller !== controller;
		}
	}

	// ---------------------------------------------------------- lifecycle ---

	function connect(port, uuid, registerEvent, _info, actionInfo) {
		const action = JSON.parse(actionInfo);
		context = action.context;
		controller = action.payload.controller || "Keypad";
		settings = Object.assign({}, config.defaults, action.payload.settings || {});

		socket = new WebSocket("ws://127.0.0.1:" + port);
		socket.addEventListener("open", () => {
			socket.send(JSON.stringify({ event: registerEvent, uuid }));
			// Save defaults so the plugin sees the same values the PI shows.
			saveSettings();
			toPlugin({ type: "init" });
		});
		socket.addEventListener("message", (message) => {
			const data = JSON.parse(message.data);
			if (data.event === "sendToPropertyInspector" && data.payload?.type === "status") {
				status = data.payload;
				renderAccount();
				renderDevices();
				onSettingsChanged();
			} else if (data.event === "didReceiveSettings") {
				settings = Object.assign({}, config.defaults, data.payload.settings || {});
				for (const input of $$("[data-setting]")) writeInput(input, settings[input.dataset.setting]);
				renderDevices();
				onSettingsChanged();
			}
		});

		applyController();
		bindSettings();
		renderAccount();
		renderDevices();
		onSettingsChanged();
	}

	// OpenDeck may call connectSocket before or after the page calls start()
	// (which happens at the end of <body>), so whichever comes second connects.
	let started = false;

	function start(options) {
		config = Object.assign(config, options);
		started = true;
		if (pendingConnection) {
			const args = pendingConnection;
			pendingConnection = null;
			connect(...args);
		}
	}

	function connectSocket(...args) {
		if (started) connect(...args);
		else pendingConnection = args;
	}

	return {
		start,
		connectSocket,
		get settings() {
			return settings;
		},
		get status() {
			return status;
		},
	};
})();

function connectOpenActionSocket(...args) {
	GoveePI.connectSocket(...args);
}
const connectElgatoStreamDeckSocket = connectOpenActionSocket;
