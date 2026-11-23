//! End-to-end test: runs the real plugin binary against a fake OpenDeck
//! (a WebSocket server speaking the OpenAction protocol) and a mock Govee API.

use std::process::Stdio;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "e2e-secret-key-9876";
const BUNDLE: &str = "io.github.nnfisfree.govee-control";
const LAMP: &str = "AA:BB:CC:DD:EE:FF:00:11";

struct FakeOpenDeck {
	ws: WebSocketStream<TcpStream>,
	_plugin: Child,
}

impl FakeOpenDeck {
	async fn start(govee_url: &str) -> Self {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		let info = json!({
			"application": { "font": "sans", "language": "en", "platform": "linux", "platformVersion": "", "version": "2.0.0" },
			"plugin": { "uuid": format!("{BUNDLE}.sdPlugin"), "version": "0.1.0" },
			"devices": [],
		});
		let plugin = Command::new(env!("CARGO_BIN_EXE_govee-control-opendeck"))
			.args(["-port", &port.to_string(), "-pluginUUID", &format!("{BUNDLE}.sdPlugin")])
			.args(["-registerEvent", "registerPlugin", "-info", &info.to_string()])
			.env("GOVEE_API_BASE_URL", govee_url)
			.stdout(Stdio::null())
			.stderr(Stdio::inherit())
			.kill_on_drop(true)
			.spawn()
			.expect("plugin binary should start");
		let (stream, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await.unwrap().unwrap();
		let ws = accept_async(stream).await.unwrap();
		Self { ws, _plugin: plugin }
	}

	async fn send(&mut self, value: Value) {
		self.ws.send(Message::Text(value.to_string().into())).await.unwrap();
	}

	/// Wait for a message matching `pred`, skipping others.
	async fn expect(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
		let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
		loop {
			let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
			let msg = tokio::time::timeout(remaining, self.ws.next())
				.await
				.unwrap_or_else(|_| panic!("timed out waiting for {what}"))
				.expect("socket closed")
				.unwrap();
			if let Message::Text(text) = msg {
				let value: Value = serde_json::from_str(&text).unwrap();
				if pred(&value) {
					return value;
				}
			}
		}
	}
}

fn lamp_ref() -> Value {
	json!({ "sku": "H6008", "device": LAMP })
}

fn appear(action: &str, context: &str, controller: &str, settings: Value) -> Value {
	json!({
		"event": "willAppear",
		"action": format!("{BUNDLE}.{action}"),
		"context": context,
		"device": "deck1",
		"payload": {
			"settings": settings,
			"coordinates": { "row": 0, "column": 0 },
			"controller": controller,
			"state": 0,
			"isInMultiAction": false
		}
	})
}

async fn mock_govee() -> MockServer {
	let server = MockServer::start().await;
	Mock::given(method("GET"))
		.and(path("/router/api/v1/user/devices"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({
			"code": 200, "message": "success",
			"data": [{
				"sku": "H6008", "device": LAMP, "deviceName": "Desk lamp", "type": "devices.types.light",
				"capabilities": [
					{ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "parameters": {} },
					{ "type": "devices.capabilities.range", "instance": "brightness", "parameters": { "range": { "min": 1, "max": 100 } } },
					{ "type": "devices.capabilities.color_setting", "instance": "colorRgb", "parameters": {} },
					{ "type": "devices.capabilities.color_setting", "instance": "colorTemperatureK", "parameters": { "range": { "min": 2700, "max": 6500 } } },
					{ "type": "devices.capabilities.dynamic_scene", "instance": "lightScene", "parameters": { "options": [] } }
				]
			}]
		})))
		.mount(&server)
		.await;
	Mock::given(method("POST"))
		.and(path("/router/api/v1/device/state"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({
			"code": 200, "msg": "success",
			"payload": { "sku": "H6008", "device": LAMP, "capabilities": [
				{ "type": "devices.capabilities.online", "instance": "online", "state": { "value": true } },
				{ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "state": { "value": 0 } },
				{ "type": "devices.capabilities.range", "instance": "brightness", "state": { "value": 40 } }
			]}
		})))
		.mount(&server)
		.await;
	Mock::given(method("POST"))
		.and(path("/router/api/v1/device/scenes"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({
			"code": 200, "msg": "success",
			"payload": { "sku": "H6008", "device": LAMP, "capabilities": [{
				"type": "devices.capabilities.dynamic_scene", "instance": "lightScene",
				"parameters": { "dataType": "ENUM", "options": [
					{ "name": "Sunrise", "value": { "paramId": 4280, "id": 3853 } },
					{ "name": "Aurora", "value": { "paramId": 4281, "id": 3854 } } ] }
			}]}
		})))
		.mount(&server)
		.await;
	Mock::given(method("POST"))
		.and(path("/router/api/v1/device/control"))
		.respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": 200, "msg": "success" })))
		.mount(&server)
		.await;
	server
}

async fn control_calls(server: &MockServer) -> Vec<Value> {
	server
		.received_requests()
		.await
		.unwrap_or_default()
		.iter()
		.filter(|r: &&Request| r.url.path() == "/router/api/v1/device/control")
		.map(|r| serde_json::from_slice::<Value>(&r.body).unwrap()["payload"]["capability"].clone())
		.collect()
}

#[tokio::test]
async fn plugin_end_to_end() {
	let govee = mock_govee().await;
	let mut deck = FakeOpenDeck::start(&govee.uri()).await;

	// Registration, then the plugin asks for its global settings.
	let reg = deck.expect("registration", |v| v["event"] == "registerPlugin").await;
	assert_eq!(reg["uuid"], format!("{BUNDLE}.sdPlugin"));
	deck.expect("getGlobalSettings", |v| v["event"] == "getGlobalSettings").await;
	deck.send(json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": { "apiKey": KEY } } })).await;

	// A power key appears; once state is fetched it shows "off" (state 0).
	deck.send(appear("power", "ctx-power", "Keypad", json!({ "devices": [lamp_ref()], "mode": "toggle" }))).await;
	deck.expect("power off state", |v| {
		v["event"] == "setState" && v["context"] == "ctx-power" && v["payload"]["state"] == 0
	})
	.await;

	// Pressing it turns the lamp on.
	deck.send(json!({
		"event": "keyDown", "action": format!("{BUNDLE}.power"), "context": "ctx-power", "device": "deck1",
		"payload": { "settings": { "devices": [lamp_ref()], "mode": "toggle" }, "coordinates": { "row": 0, "column": 0 },
			"controller": "Keypad", "state": 0, "isInMultiAction": false }
	}))
	.await;
	deck.expect("power on state", |v| {
		v["event"] == "setState" && v["context"] == "ctx-power" && v["payload"]["state"] == 1
	})
	.await;
	tokio::time::sleep(Duration::from_millis(200)).await;
	assert_eq!(
		control_calls(&govee).await,
		vec![json!({ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "value": 1 })]
	);

	// A brightness dial shows 40%, then two quick turns of +3 ticks (5% each)
	// produce one debounced API call for 70%.
	let dial_settings = json!({ "devices": [lamp_ref()], "dialStep": 5 });
	deck.send(appear("brightness", "ctx-dial", "Encoder", dial_settings.clone())).await;
	deck.expect("dial feedback 40%", |v| v["event"] == "setFeedback" && v["payload"]["value"] == "40%").await;
	for _ in 0..2 {
		deck.send(json!({
			"event": "dialRotate", "action": format!("{BUNDLE}.brightness"), "context": "ctx-dial", "device": "deck1",
			"payload": { "settings": dial_settings, "coordinates": { "row": 0, "column": 0 }, "ticks": 3, "pressed": false }
		}))
		.await;
	}
	deck.expect("dial feedback 70%", |v| v["event"] == "setFeedback" && v["payload"]["value"] == "70%").await;
	tokio::time::sleep(Duration::from_millis(600)).await;
	let calls = control_calls(&govee).await;
	assert_eq!(calls.len(), 2, "expected one power call and one debounced brightness call, got {calls:?}");
	assert_eq!(calls[1]["instance"], "brightness");
	assert_eq!(calls[1]["value"], 70);

	// The property inspector gets the device list and only a masked key.
	deck.send(json!({
		"event": "sendToPlugin", "action": format!("{BUNDLE}.brightness"), "context": "ctx-dial",
		"payload": { "type": "init" }
	}))
	.await;
	let status = deck.expect("PI status", |v| v["event"] == "sendToPropertyInspector").await;
	let payload = &status["payload"];
	assert_eq!(payload["type"], "status");
	assert_eq!(payload["hasKey"], true);
	assert_eq!(payload["keyHint"], "••••9876");
	assert_eq!(payload["devices"][0]["name"], "Desk lamp");
	assert_eq!(payload["devices"][0]["features"]["kelvinMin"], 2700);
	assert!(!status.to_string().contains(KEY), "API key leaked to the property inspector");

	// The colour key draws its colour as an SVG image.
	deck.send(appear("color", "ctx-color", "Keypad", json!({ "devices": [lamp_ref()], "color": "#00ff00" }))).await;
	let image = deck.expect("colour image", |v| v["event"] == "setImage" && v["context"] == "ctx-color").await;
	assert!(image["payload"]["image"].as_str().unwrap().starts_with("data:image/svg+xml;base64,"));

	// A scene key looks its scene up by name and sends that light's value.
	let scene_settings = json!({ "devices": [lamp_ref()], "source": "scene", "scene": "aurora" });
	deck.send(appear("scene", "ctx-scene", "Keypad", scene_settings.clone())).await;
	deck.send(key_event("keyDown", "scene", "ctx-scene", scene_settings)).await;
	tokio::time::sleep(Duration::from_millis(500)).await;
	let calls = control_calls(&govee).await;
	assert_eq!(
		calls.last().unwrap(),
		&json!({ "type": "devices.capabilities.dynamic_scene", "instance": "lightScene", "value": { "paramId": 4281, "id": 3854 } })
	);

	// The settings panel can list the scenes.
	deck.send(json!({
		"event": "sendToPlugin", "action": format!("{BUNDLE}.scene"), "context": "ctx-scene",
		"payload": { "type": "scenes", "source": "scene", "devices": [lamp_ref()] }
	}))
	.await;
	let scenes = deck
		.expect("scene list", |v| v["event"] == "sendToPropertyInspector" && v["payload"]["type"] == "scenes")
		.await;
	assert_eq!(scenes["payload"]["names"], json!(["Sunrise", "Aurora"]));

	// Long press: holding a power key set to "turn off when held" turns the
	// lamp off, and letting go afterwards sends nothing more.
	let hold_settings = json!({ "devices": [lamp_ref()], "mode": "on", "holdAction": "off" });
	deck.send(appear("power", "ctx-hold", "Keypad", hold_settings.clone())).await;
	let before = control_calls(&govee).await.len();
	deck.send(key_event("keyDown", "power", "ctx-hold", hold_settings.clone())).await;
	tokio::time::sleep(Duration::from_millis(800)).await;
	deck.send(key_event("keyUp", "power", "ctx-hold", hold_settings.clone())).await;
	tokio::time::sleep(Duration::from_millis(300)).await;
	let calls = control_calls(&govee).await;
	assert_eq!(calls.len(), before + 1, "expected only the long-press call, got {:?}", &calls[before..]);
	assert_eq!(calls[before]["value"], 0);

	// A short press on the same key does the normal action when the key is released.
	deck.send(key_event("keyDown", "power", "ctx-hold", hold_settings.clone())).await;
	tokio::time::sleep(Duration::from_millis(100)).await;
	assert_eq!(control_calls(&govee).await.len(), before + 1, "nothing is sent until the key is released");
	deck.send(key_event("keyUp", "power", "ctx-hold", hold_settings)).await;
	tokio::time::sleep(Duration::from_millis(300)).await;
	let calls = control_calls(&govee).await;
	assert_eq!(calls.len(), before + 2);
	assert_eq!(calls[before + 1]["value"], 1);
}

fn key_event(event: &str, action: &str, context: &str, settings: Value) -> Value {
	json!({
		"event": event, "action": format!("{BUNDLE}.{action}"), "context": context, "device": "deck1",
		"payload": { "settings": settings, "coordinates": { "row": 0, "column": 0 },
			"controller": "Keypad", "state": 0, "isInMultiAction": false }
	})
}
