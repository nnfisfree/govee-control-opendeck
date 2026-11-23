use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use super::ratelimit::{Limit, RateLimiter};
use super::types::{Device, DeviceRef, DeviceState, NamedValue, SceneKind, StateCapability, cap, options_of};

pub const DEFAULT_BASE_URL: &str = "https://openapi.api.govee.com";
const USER_AGENT: &str = concat!("govee-control-opendeck/", env!("CARGO_PKG_VERSION"));
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(1);

/// Errors from the Govee API. None of the messages contain the API key.
#[derive(Debug, thiserror::Error)]
pub enum GoveeError {
	#[error("the Govee API key was rejected (check it or request a new one)")]
	Unauthorized,
	#[error("Govee rate limit reached, try again shortly")]
	RateLimited,
	#[error("Govee API error {code}: {message}")]
	Api { code: i64, message: String },
	#[error("network error talking to Govee: {0}")]
	Http(#[from] reqwest::Error),
	#[error("unexpected response from Govee: {0}")]
	Decode(String),
}

/// A control command, mapped to a Govee capability.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
	Power(bool),
	/// Brightness in percent, clamped to 1..=100.
	Brightness(u8),
	/// Packed `0xRRGGBB`.
	Rgb(u32),
	Kelvin(u32),
	/// Play a scene. `value` is the option value from the scene list, which
	/// differs between models, so look it up per device.
	Scene {
		kind: SceneKind,
		value: Value,
	},
	/// Music-reactive mode. `rgb` is only used when `auto_color` is off.
	Music {
		mode: i64,
		sensitivity: u8,
		auto_color: bool,
		rgb: u32,
	},
	/// Colour some segments (0-based) of a strip or bar.
	SegmentRgb {
		segments: Vec<u32>,
		rgb: u32,
	},
	/// Brightness of some segments (0-based), 0..=100.
	SegmentBrightness {
		segments: Vec<u32>,
		brightness: u8,
	},
	/// Switch a `toggle` capability such as `gradientToggle`.
	Toggle {
		instance: String,
		on: bool,
	},
	/// Appliance work mode, e.g. a fan's gear and speed.
	WorkMode {
		mode: i64,
		value: i64,
	},
	/// Heater or kettle target temperature.
	TargetTemperature {
		temperature: i64,
		fahrenheit: bool,
	},
}

impl Command {
	pub fn capability(&self) -> Value {
		let (kind, instance, value) = match self {
			Command::Power(on) => (cap::ON_OFF, cap::POWER_SWITCH, json!(*on as u8)),
			Command::Brightness(b) => (cap::RANGE, cap::BRIGHTNESS, json!((*b).clamp(1, 100))),
			Command::Rgb(rgb) => (cap::COLOR_SETTING, cap::COLOR_RGB, json!(rgb & 0xff_ffff)),
			Command::Kelvin(k) => (cap::COLOR_SETTING, cap::COLOR_TEMPERATURE_K, json!(k)),
			Command::Scene { kind, value } => (cap::DYNAMIC_SCENE, kind.instance(), value.clone()),
			Command::Music { mode, sensitivity, auto_color, rgb } => {
				let mut value = json!({
					"musicMode": mode,
					"sensitivity": (*sensitivity).min(100),
					"autoColor": *auto_color as u8,
				});
				if !auto_color {
					value["rgb"] = json!(rgb & 0xff_ffff);
				}
				(cap::MUSIC_SETTING, cap::MUSIC_MODE, value)
			}
			Command::SegmentRgb { segments, rgb } => {
				(cap::SEGMENT_COLOR, cap::SEGMENTED_RGB, json!({ "segment": segments, "rgb": rgb & 0xff_ffff }))
			}
			Command::SegmentBrightness { segments, brightness } => (
				cap::SEGMENT_COLOR,
				cap::SEGMENTED_BRIGHTNESS,
				json!({ "segment": segments, "brightness": (*brightness).min(100) }),
			),
			Command::Toggle { instance, on } => (cap::TOGGLE, instance.as_str(), json!(*on as u8)),
			Command::WorkMode { mode, value } => {
				(cap::WORK_MODE, cap::WORK_MODE_INSTANCE, json!({ "workMode": mode, "modeValue": value }))
			}
			Command::TargetTemperature { temperature, fahrenheit } => (
				cap::TEMPERATURE_SETTING,
				cap::TARGET_TEMPERATURE,
				json!({
					"autoStop": 0,
					"temperature": temperature,
					"unit": if *fahrenheit { "Fahrenheit" } else { "Celsius" },
				}),
			),
		};
		json!({ "type": kind, "instance": instance, "value": value })
	}

	/// Apply this command to a cached state (optimistic update after success).
	pub fn apply_to(&self, state: &mut DeviceState) {
		match self {
			Command::Power(on) => state.power = Some(*on),
			Command::Brightness(b) => {
				state.brightness = Some((*b).clamp(1, 100));
			}
			Command::Rgb(rgb) => {
				state.rgb = Some(*rgb);
				state.kelvin = None;
			}
			Command::Kelvin(k) => state.kelvin = Some(*k),
			// Govee turns a light on when it starts a scene or music mode.
			Command::Scene { .. } | Command::Music { .. } => state.power = Some(true),
			Command::SegmentRgb { .. } | Command::SegmentBrightness { .. } => {}
			Command::Toggle { instance, on } => {
				state.toggles.insert(instance.clone(), *on);
			}
			Command::WorkMode { mode, value } => state.work_mode = Some((*mode, *value)),
			Command::TargetTemperature { temperature, .. } => state.target_temperature = Some(*temperature as f64),
		}
	}
}

/// Thin async client for the Govee Platform API with built-in rate limiting.
pub struct GoveeClient {
	http: reqwest::Client,
	base_url: String,
	api_key: String,
	limiter: Arc<RateLimiter>,
}

impl std::fmt::Debug for GoveeClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GoveeClient").field("base_url", &self.base_url).field("api_key", &"<redacted>").finish()
	}
}

#[derive(Deserialize)]
struct DevicesResponse {
	#[serde(default)]
	data: Vec<Device>,
}

#[derive(Deserialize)]
struct StatePayload {
	#[serde(default)]
	capabilities: Vec<StateCapability>,
}

impl GoveeClient {
	/// Client for the real Govee API. Debug builds honour `GOVEE_API_BASE_URL`
	/// so end-to-end tests can point the plugin at a mock server. Release
	/// builds always use [`DEFAULT_BASE_URL`].
	pub fn new(api_key: impl Into<String>) -> Result<Self, GoveeError> {
		let base_url = match std::env::var("GOVEE_API_BASE_URL") {
			Ok(url) if cfg!(debug_assertions) && !url.is_empty() => url,
			_ => DEFAULT_BASE_URL.to_owned(),
		};
		Self::with_base_url(api_key, base_url)
	}

	pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, GoveeError> {
		let http = reqwest::Client::builder().user_agent(USER_AGENT).timeout(REQUEST_TIMEOUT).build()?;
		Ok(Self {
			http,
			base_url: base_url.into().trim_end_matches('/').to_owned(),
			api_key: api_key.into().trim().to_owned(),
			limiter: Arc::new(RateLimiter::default()),
		})
	}

	pub fn api_key(&self) -> &str {
		&self.api_key
	}

	/// List all devices on the account with their capabilities.
	pub async fn devices(&self) -> Result<Vec<Device>, GoveeError> {
		self.limiter.acquire(Limit::DeviceList, "").await;
		let body = self.send(|| self.http.get(self.url("/router/api/v1/user/devices"))).await?;
		let parsed: DevicesResponse = serde_json::from_value(body).map_err(|e| GoveeError::Decode(e.to_string()))?;
		Ok(parsed.data)
	}

	/// Query the current state of one device.
	pub async fn state(&self, device: &DeviceRef) -> Result<DeviceState, GoveeError> {
		self.limiter.acquire(Limit::State, &device.device).await;
		let request = json!({
			"requestId": uuid::Uuid::new_v4().to_string(),
			"payload": { "sku": device.sku, "device": device.device },
		});
		let body = self.send(|| self.http.post(self.url("/router/api/v1/device/state")).json(&request)).await?;
		let payload = body.get("payload").cloned().unwrap_or(Value::Null);
		let parsed: StatePayload = serde_json::from_value(payload).map_err(|e| GoveeError::Decode(e.to_string()))?;
		Ok(DeviceState::from_capabilities(&parsed.capabilities))
	}

	/// Send one control command to one device.
	pub async fn control(&self, device: &DeviceRef, command: &Command) -> Result<(), GoveeError> {
		self.limiter.acquire(Limit::ControlDevice, &device.device).await;
		self.limiter.acquire(Limit::ControlAccount, "").await;
		let request = json!({
			"requestId": uuid::Uuid::new_v4().to_string(),
			"payload": { "sku": device.sku, "device": device.device, "capability": command.capability() },
		});
		self.send(|| self.http.post(self.url("/router/api/v1/device/control")).json(&request)).await?;
		Ok(())
	}

	/// List the built-in or DIY scenes of one device. Snapshots are not
	/// fetched here: they come with the device list ([`Device::snapshots`]).
	pub async fn scenes(&self, device: &DeviceRef, kind: SceneKind) -> Result<Vec<NamedValue>, GoveeError> {
		let path = match kind {
			SceneKind::Scene => "/router/api/v1/device/scenes",
			SceneKind::Diy => "/router/api/v1/device/diy-scenes",
			SceneKind::Snapshot => return Ok(Vec::new()),
		};
		self.limiter.acquire(Limit::SceneList, &device.device).await;
		let request = json!({
			"requestId": uuid::Uuid::new_v4().to_string(),
			"payload": { "sku": device.sku, "device": device.device },
		});
		let body = self.send(|| self.http.post(self.url(path)).json(&request)).await?;
		let capabilities = body.pointer("/payload/capabilities").and_then(Value::as_array).cloned().unwrap_or_default();
		Ok(capabilities
			.iter()
			.filter(|c| c.get("instance").and_then(Value::as_str) == Some(kind.instance()))
			.flat_map(|c| options_of(c.get("parameters").unwrap_or(&Value::Null)))
			.collect())
	}

	fn url(&self, path: &str) -> String {
		format!("{}{}", self.base_url, path)
	}

	/// Send a request, retrying once after a 429, and check both the HTTP
	/// status and the `code` field Govee puts in the JSON body.
	async fn send(&self, build: impl Fn() -> reqwest::RequestBuilder) -> Result<Value, GoveeError> {
		let mut retried = false;
		loop {
			let response = build().header("Govee-API-Key", &self.api_key).send().await?;
			let status = response.status();

			if status == StatusCode::TOO_MANY_REQUESTS {
				if retried {
					return Err(GoveeError::RateLimited);
				}
				let wait = response
					.headers()
					.get(reqwest::header::RETRY_AFTER)
					.and_then(|v| v.to_str().ok())
					.and_then(|v| v.trim().parse::<u64>().ok())
					.map(|s| Duration::from_secs(s.min(10)))
					.unwrap_or(DEFAULT_RETRY_AFTER);
				log::warn!("Govee returned 429; retrying in {wait:?}");
				tokio::time::sleep(wait).await;
				retried = true;
				continue;
			}
			if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
				return Err(GoveeError::Unauthorized);
			}

			let text = response.text().await?;
			let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
			let message =
				body.get("msg").or_else(|| body.get("message")).and_then(Value::as_str).unwrap_or("").to_owned();

			if !status.is_success() {
				let message = if message.is_empty() { status.to_string() } else { message };
				return Err(GoveeError::Api { code: status.as_u16() as i64, message });
			}
			if body.is_null() {
				return Err(GoveeError::Decode("response body is not JSON".into()));
			}
			match body.get("code").and_then(Value::as_i64) {
				None | Some(200) => return Ok(body),
				Some(401) => return Err(GoveeError::Unauthorized),
				Some(429) => return Err(GoveeError::RateLimited),
				Some(code) => return Err(GoveeError::Api { code, message }),
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use wiremock::matchers::{body_partial_json, header, method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	const KEY: &str = "test-key-123";

	fn client(server: &MockServer) -> GoveeClient {
		GoveeClient::with_base_url(KEY, server.uri()).unwrap()
	}

	fn lamp() -> DeviceRef {
		DeviceRef { sku: "H6601".into(), device: "AA:BB".into() }
	}

	#[test]
	fn command_capabilities() {
		assert_eq!(
			Command::Power(true).capability(),
			json!({ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "value": 1 })
		);
		assert_eq!(Command::Brightness(0).capability()["value"], json!(1));
		assert_eq!(Command::Brightness(250).capability()["value"], json!(100));
		assert_eq!(
			Command::Rgb(0xff8800).capability(),
			json!({ "type": "devices.capabilities.color_setting", "instance": "colorRgb", "value": 16746496 })
		);
		assert_eq!(Command::Kelvin(4000).capability()["instance"], json!("colorTemperatureK"));
	}

	#[test]
	fn extended_command_capabilities() {
		assert_eq!(
			Command::Scene { kind: SceneKind::Scene, value: json!({ "paramId": 1, "id": 2 }) }.capability(),
			json!({ "type": "devices.capabilities.dynamic_scene", "instance": "lightScene", "value": { "paramId": 1, "id": 2 } })
		);
		assert_eq!(
			Command::Music { mode: 5, sensitivity: 120, auto_color: true, rgb: 0 }.capability()["value"],
			json!({ "musicMode": 5, "sensitivity": 100, "autoColor": 1 })
		);
		assert_eq!(
			Command::Music { mode: 3, sensitivity: 50, auto_color: false, rgb: 0xff0000 }.capability()["value"]["rgb"],
			json!(0xff0000)
		);
		assert_eq!(
			Command::SegmentRgb { segments: vec![0, 1], rgb: 0x00ff00 }.capability(),
			json!({ "type": "devices.capabilities.segment_color_setting", "instance": "segmentedColorRgb",
				"value": { "segment": [0, 1], "rgb": 0x00ff00 } })
		);
		assert_eq!(
			Command::Toggle { instance: "gradientToggle".into(), on: true }.capability(),
			json!({ "type": "devices.capabilities.toggle", "instance": "gradientToggle", "value": 1 })
		);
		assert_eq!(
			Command::WorkMode { mode: 1, value: 2 }.capability()["value"],
			json!({ "workMode": 1, "modeValue": 2 })
		);
		assert_eq!(
			Command::TargetTemperature { temperature: 22, fahrenheit: false }.capability()["value"],
			json!({ "autoStop": 0, "temperature": 22, "unit": "Celsius" })
		);
		let mut state = DeviceState::default();
		Command::Toggle { instance: "nightlightToggle".into(), on: true }.apply_to(&mut state);
		Command::Scene { kind: SceneKind::Diy, value: json!(1) }.apply_to(&mut state);
		assert_eq!(state.toggles.get("nightlightToggle"), Some(&true));
		assert_eq!(state.power, Some(true));
	}

	#[tokio::test]
	async fn lists_scenes() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/router/api/v1/device/scenes"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"code": 200, "msg": "success",
				"payload": { "sku": "H6601", "device": "AA:BB", "capabilities": [{
					"type": "devices.capabilities.dynamic_scene", "instance": "lightScene",
					"parameters": { "dataType": "ENUM", "options": [
						{ "name": "Sunrise", "value": { "paramId": 4280, "id": 3853 } },
						{ "name": "Aurora", "value": { "paramId": 4281, "id": 3854 } } ] }
				}]}
			})))
			.mount(&server)
			.await;
		let scenes = client(&server).scenes(&lamp(), SceneKind::Scene).await.unwrap();
		assert_eq!(scenes.len(), 2);
		assert_eq!(scenes[0].name, "Sunrise");
		assert_eq!(scenes[1].value, json!({ "paramId": 4281, "id": 3854 }));
		assert!(client(&server).scenes(&lamp(), SceneKind::Snapshot).await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn lists_devices_with_api_key_header() {
		let server = MockServer::start().await;
		Mock::given(method("GET"))
			.and(path("/router/api/v1/user/devices"))
			.and(header("Govee-API-Key", KEY))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"code": 200, "message": "success",
				"data": [{ "sku": "H6601", "device": "AA:BB", "deviceName": "Lamp", "capabilities": [] }]
			})))
			.expect(1)
			.mount(&server)
			.await;

		let devices = client(&server).devices().await.unwrap();
		assert_eq!(devices.len(), 1);
		assert_eq!(devices[0].name, "Lamp");
	}

	#[tokio::test]
	async fn sends_control_payload() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/router/api/v1/device/control"))
			.and(body_partial_json(json!({
				"payload": { "sku": "H6601", "device": "AA:BB",
					"capability": { "type": "devices.capabilities.range", "instance": "brightness", "value": 42 } }
			})))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": 200, "msg": "success" })))
			.expect(1)
			.mount(&server)
			.await;

		client(&server).control(&lamp(), &Command::Brightness(42)).await.unwrap();
	}

	#[tokio::test]
	async fn parses_state() {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/router/api/v1/device/state"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"requestId": "x", "msg": "success", "code": 200,
				"payload": { "sku": "H6601", "device": "AA:BB", "capabilities": [
					{ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "state": { "value": 0 } },
					{ "type": "devices.capabilities.range", "instance": "brightness", "state": { "value": 77 } }
				]}
			})))
			.mount(&server)
			.await;

		let state = client(&server).state(&lamp()).await.unwrap();
		assert_eq!(state.power, Some(false));
		assert_eq!(state.brightness, Some(77));
	}

	#[tokio::test]
	async fn maps_errors() {
		let server = MockServer::start().await;
		Mock::given(path("/router/api/v1/user/devices")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
		Mock::given(path("/router/api/v1/device/control"))
			.respond_with(
				ResponseTemplate::new(200).set_body_json(json!({ "code": 400, "msg": "Parameter value out of range" })),
			)
			.mount(&server)
			.await;

		let c = client(&server);
		assert!(matches!(c.devices().await, Err(GoveeError::Unauthorized)));
		match c.control(&lamp(), &Command::Kelvin(1)).await {
			Err(GoveeError::Api { code: 400, message }) => assert_eq!(message, "Parameter value out of range"),
			other => panic!("unexpected {other:?}"),
		}
		// The key must never leak into error text or debug output.
		assert!(!format!("{c:?}").contains(KEY));
	}

	#[tokio::test]
	async fn retries_once_after_429() {
		let server = MockServer::start().await;
		Mock::given(path("/router/api/v1/device/control"))
			.respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
			.up_to_n_times(1)
			.mount(&server)
			.await;
		Mock::given(path("/router/api/v1/device/control"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({ "code": 200 })))
			.mount(&server)
			.await;

		client(&server).control(&lamp(), &Command::Power(true)).await.unwrap();
	}
}
