use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability `type` / `instance` pairs used by this plugin.
pub mod cap {
	pub const ON_OFF: &str = "devices.capabilities.on_off";
	pub const RANGE: &str = "devices.capabilities.range";
	pub const COLOR_SETTING: &str = "devices.capabilities.color_setting";
	pub const ONLINE: &str = "devices.capabilities.online";
	pub const DYNAMIC_SCENE: &str = "devices.capabilities.dynamic_scene";
	pub const MUSIC_SETTING: &str = "devices.capabilities.music_setting";
	pub const SEGMENT_COLOR: &str = "devices.capabilities.segment_color_setting";
	pub const TOGGLE: &str = "devices.capabilities.toggle";
	pub const WORK_MODE: &str = "devices.capabilities.work_mode";
	pub const TEMPERATURE_SETTING: &str = "devices.capabilities.temperature_setting";
	pub const PROPERTY: &str = "devices.capabilities.property";

	pub const POWER_SWITCH: &str = "powerSwitch";
	pub const BRIGHTNESS: &str = "brightness";
	pub const COLOR_RGB: &str = "colorRgb";
	pub const COLOR_TEMPERATURE_K: &str = "colorTemperatureK";
	pub const ONLINE_INSTANCE: &str = "online";
	pub const LIGHT_SCENE: &str = "lightScene";
	pub const DIY_SCENE: &str = "diyScene";
	pub const SNAPSHOT: &str = "snapshot";
	pub const MUSIC_MODE: &str = "musicMode";
	pub const SEGMENTED_RGB: &str = "segmentedColorRgb";
	pub const SEGMENTED_BRIGHTNESS: &str = "segmentedBrightness";
	pub const WORK_MODE_INSTANCE: &str = "workMode";
	pub const TARGET_TEMPERATURE: &str = "targetTemperature";
	pub const SENSOR_TEMPERATURE: &str = "sensorTemperature";
	pub const SENSOR_HUMIDITY: &str = "sensorHumidity";
}

/// Default colour-temperature range when a device does not report one.
pub const DEFAULT_KELVIN_RANGE: (u32, u32) = (2000, 9000);

/// Identifies one Govee device. Both fields are needed by every API call.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceRef {
	pub sku: String,
	pub device: String,
}

/// One entry from `GET /router/api/v1/user/devices`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Device {
	pub sku: String,
	pub device: String,
	#[serde(default, rename = "deviceName")]
	pub name: String,
	#[serde(default, rename = "type")]
	pub kind: String,
	#[serde(default)]
	pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Capability {
	#[serde(rename = "type")]
	pub kind: String,
	pub instance: String,
	#[serde(default)]
	pub parameters: Value,
}

/// A named option from an `ENUM` capability or a scene list.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NamedValue {
	pub name: String,
	#[serde(default)]
	pub value: Value,
}

/// The three kinds of `dynamic_scene` a light can play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SceneKind {
	/// Govee's built-in scenes (`lightScene`), listed by `/device/scenes`.
	#[default]
	Scene,
	/// Scenes the user made in the Govee app (`diyScene`), listed by `/device/diy-scenes`.
	Diy,
	/// Snapshots saved in the Govee app, listed in the device's capabilities.
	Snapshot,
}

impl SceneKind {
	pub fn instance(self) -> &'static str {
		match self {
			SceneKind::Scene => cap::LIGHT_SCENE,
			SceneKind::Diy => cap::DIY_SCENE,
			SceneKind::Snapshot => cap::SNAPSHOT,
		}
	}
}

/// Which plugin features a device supports, derived from its capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
	pub power: bool,
	pub brightness: bool,
	pub rgb: bool,
	pub kelvin: bool,
	pub kelvin_min: u32,
	pub kelvin_max: u32,
	/// Brightness, colour or colour temperature: what presets and cycles need.
	pub light: bool,
	/// Any kind of scene (built-in, DIY or snapshot).
	pub scene: bool,
	pub light_scenes: bool,
	pub diy_scenes: bool,
	pub snapshots: bool,
	pub music: bool,
	/// Number of individually addressable segments (0 if none).
	pub segments: u32,
	pub segment_brightness: bool,
	pub toggle: bool,
	/// Temperature or humidity readings (thermometers, hygrometers).
	pub sensor: bool,
	pub work_mode: bool,
	pub target_temperature: bool,
	/// Work modes or a target temperature: heaters, fans, purifiers, humidifiers…
	pub appliance: bool,
}

impl Device {
	pub fn reference(&self) -> DeviceRef {
		DeviceRef { sku: self.sku.clone(), device: self.device.clone() }
	}

	pub fn capability(&self, kind: &str, instance: &str) -> Option<&Capability> {
		// Match on the instance and only loosely on the type: Govee's docs and
		// responses are not always consistent about the type string.
		self.capabilities
			.iter()
			.find(|c| c.instance == instance && c.kind == kind)
			.or_else(|| self.capabilities.iter().find(|c| c.instance == instance))
	}

	/// Instances of every `toggle` capability, e.g. `gradientToggle`.
	pub fn toggles(&self) -> Vec<&str> {
		self.capabilities.iter().filter(|c| c.kind == cap::TOGGLE).map(|c| c.instance.as_str()).collect()
	}

	pub fn features(&self) -> Features {
		let kelvin = self.capability(cap::COLOR_SETTING, cap::COLOR_TEMPERATURE_K);
		let (kelvin_min, kelvin_max) = kelvin
			.and_then(Capability::range)
			.unwrap_or((DEFAULT_KELVIN_RANGE.0 as i64, DEFAULT_KELVIN_RANGE.1 as i64));
		let has = |kind, instance| self.capability(kind, instance).is_some();
		let brightness = has(cap::RANGE, cap::BRIGHTNESS);
		let rgb = has(cap::COLOR_SETTING, cap::COLOR_RGB);
		let light_scenes = has(cap::DYNAMIC_SCENE, cap::LIGHT_SCENE);
		let diy_scenes = has(cap::DYNAMIC_SCENE, cap::DIY_SCENE);
		let snapshots = has(cap::DYNAMIC_SCENE, cap::SNAPSHOT);
		let work_mode = has(cap::WORK_MODE, cap::WORK_MODE_INSTANCE);
		let target_temperature = has(cap::TEMPERATURE_SETTING, cap::TARGET_TEMPERATURE);
		Features {
			power: has(cap::ON_OFF, cap::POWER_SWITCH),
			brightness,
			rgb,
			kelvin: kelvin.is_some(),
			kelvin_min: kelvin_min.max(0) as u32,
			kelvin_max: kelvin_max.max(0) as u32,
			light: brightness || rgb || kelvin.is_some(),
			scene: light_scenes || diy_scenes || snapshots,
			light_scenes,
			diy_scenes,
			snapshots,
			music: has(cap::MUSIC_SETTING, cap::MUSIC_MODE),
			segments: self.segment_count(),
			segment_brightness: has(cap::SEGMENT_COLOR, cap::SEGMENTED_BRIGHTNESS),
			toggle: !self.toggles().is_empty(),
			sensor: has(cap::PROPERTY, cap::SENSOR_TEMPERATURE) || has(cap::PROPERTY, cap::SENSOR_HUMIDITY),
			work_mode,
			target_temperature,
			appliance: work_mode || target_temperature,
		}
	}

	/// Number of segments, from the `segment` field's element range.
	pub fn segment_count(&self) -> u32 {
		let Some(c) = self.capability(cap::SEGMENT_COLOR, cap::SEGMENTED_RGB) else { return 0 };
		let Some(field) = c.field("segment") else { return 0 };
		let from_range = field.get("elementRange").and_then(|r| r.get("max")).and_then(Value::as_u64).map(|m| m + 1);
		let from_size = field.get("size").and_then(|r| r.get("max")).and_then(Value::as_u64);
		from_range.or(from_size).unwrap_or(0).min(256) as u32
	}

	/// The options of the `musicMode` field (name and numeric value).
	pub fn music_modes(&self) -> Vec<NamedValue> {
		self.capability(cap::MUSIC_SETTING, cap::MUSIC_MODE)
			.and_then(|c| c.field(cap::MUSIC_MODE))
			.map(options_of)
			.unwrap_or_default()
	}

	/// Snapshots saved in the Govee app.
	pub fn snapshots(&self) -> Vec<NamedValue> {
		self.capability(cap::DYNAMIC_SCENE, cap::SNAPSHOT).map(|c| options_of(&c.parameters)).unwrap_or_default()
	}

	/// True if the device reports sensor temperatures in Celsius. Govee's
	/// thermometers report Fahrenheit unless the capability says otherwise.
	pub fn sensor_celsius(&self) -> bool {
		self.capability(cap::PROPERTY, cap::SENSOR_TEMPERATURE)
			.and_then(|c| c.parameters.get("unit"))
			.and_then(Value::as_str)
			.is_some_and(|u| u.to_ascii_lowercase().contains("celsius"))
	}

	/// `(min, max)` of the target temperature, if the device reports one.
	pub fn target_temperature_range(&self) -> Option<(i64, i64)> {
		let c = self.capability(cap::TEMPERATURE_SETTING, cap::TARGET_TEMPERATURE)?;
		let field = c.field("temperature")?;
		let range = field.get("range")?;
		Some((range.get("min")?.as_i64()?, range.get("max")?.as_i64()?))
	}

	/// Human-friendly name, falling back to the SKU.
	pub fn display_name(&self) -> &str {
		if self.name.trim().is_empty() { &self.sku } else { &self.name }
	}
}

impl Capability {
	/// `(min, max)` from `parameters.range`, if present.
	pub fn range(&self) -> Option<(i64, i64)> {
		let range = self.parameters.get("range")?;
		Some((range.get("min")?.as_i64()?, range.get("max")?.as_i64()?))
	}

	/// One field of a `STRUCT` capability, by `fieldName`.
	pub fn field(&self, name: &str) -> Option<&Value> {
		self.parameters
			.get("fields")?
			.as_array()?
			.iter()
			.find(|f| f.get("fieldName").and_then(Value::as_str) == Some(name))
	}
}

/// `options` of a capability's parameters (or of one of its fields).
pub fn options_of(parameters: &Value) -> Vec<NamedValue> {
	parameters
		.get("options")
		.and_then(Value::as_array)
		.map(|options| options.iter().filter_map(|o| serde_json::from_value(o.clone()).ok()).collect())
		.unwrap_or_default()
}

/// Last known state of one device (from `/device/state` or optimistic updates).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceState {
	pub online: Option<bool>,
	pub power: Option<bool>,
	pub brightness: Option<u8>,
	pub rgb: Option<u32>,
	pub kelvin: Option<u32>,
	/// State of each `toggle` capability, by instance.
	pub toggles: BTreeMap<String, bool>,
	/// As reported by the device (usually Fahrenheit, see [`Device::sensor_celsius`]).
	pub sensor_temperature: Option<f64>,
	pub sensor_humidity: Option<f64>,
	/// `(workMode, modeValue)`.
	pub work_mode: Option<(i64, i64)>,
	pub target_temperature: Option<f64>,
}

/// Raw capability entry inside a `/device/state` response.
#[derive(Clone, Debug, Deserialize)]
pub struct StateCapability {
	#[serde(default, rename = "type")]
	pub kind: String,
	pub instance: String,
	#[serde(default)]
	pub state: Value,
}

impl DeviceState {
	pub fn from_capabilities(capabilities: &[StateCapability]) -> Self {
		let mut state = DeviceState::default();
		for c in capabilities {
			let value = c.state.get("value").unwrap_or(&Value::Null);
			match c.instance.as_str() {
				cap::ONLINE_INSTANCE => state.online = value_as_bool(value),
				cap::POWER_SWITCH => state.power = value_as_bool(value),
				cap::BRIGHTNESS => state.brightness = value_as_i64(value).map(|v| v.clamp(0, 100) as u8),
				cap::COLOR_RGB => state.rgb = value_as_i64(value).map(|v| (v.max(0) as u32) & 0xff_ffff),
				// Govee reports 0 when the light is in RGB mode.
				cap::COLOR_TEMPERATURE_K => state.kelvin = value_as_i64(value).filter(|v| *v > 0).map(|v| v as u32),
				cap::SENSOR_TEMPERATURE => state.sensor_temperature = value_as_f64(value),
				cap::SENSOR_HUMIDITY => {
					// Some sensors report `{ "currentHumidity": 45 }` instead of a number.
					state.sensor_humidity =
						value_as_f64(value).or_else(|| value.get("currentHumidity").and_then(value_as_f64))
				}
				cap::WORK_MODE_INSTANCE => {
					let mode = value.get("workMode").and_then(value_as_i64);
					let mode_value = value.get("modeValue").and_then(value_as_i64).unwrap_or(0);
					state.work_mode = mode.map(|m| (m, mode_value));
				}
				cap::TARGET_TEMPERATURE => {
					state.target_temperature =
						value.get("temperature").and_then(value_as_f64).or_else(|| value_as_f64(value))
				}
				instance if c.kind == cap::TOGGLE => {
					if let Some(on) = value_as_bool(value) {
						state.toggles.insert(instance.to_owned(), on);
					}
				}
				_ => {}
			}
		}
		state
	}
}

fn value_as_i64(value: &Value) -> Option<i64> {
	match value {
		Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
		Value::String(s) => s.trim().parse().ok(),
		Value::Bool(b) => Some(*b as i64),
		_ => None,
	}
}

fn value_as_f64(value: &Value) -> Option<f64> {
	match value {
		Value::Number(n) => n.as_f64(),
		Value::String(s) => s.trim().parse().ok(),
		_ => None,
	}
}

fn value_as_bool(value: &Value) -> Option<bool> {
	match value {
		Value::Bool(b) => Some(*b),
		Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
		Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
		other => value_as_i64(other).map(|v| v != 0),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn sample_device() -> Device {
		serde_json::from_value(json!({
			"sku": "H6601",
			"device": "AA:BB:CC:DD:EE:FF:00:01",
			"deviceName": "Desk lamp",
			"type": "devices.types.light",
			"capabilities": [
				{ "type": "devices.capabilities.on_off", "instance": "powerSwitch",
				  "parameters": { "dataType": "ENUM", "options": [ { "name": "on", "value": 1 }, { "name": "off", "value": 0 } ] } },
				{ "type": "devices.capabilities.range", "instance": "brightness",
				  "parameters": { "unit": "unit.percent", "dataType": "INTEGER", "range": { "min": 1, "max": 100, "precision": 1 } } },
				{ "type": "devices.capabilities.color_setting", "instance": "colorRgb",
				  "parameters": { "dataType": "INTEGER", "range": { "min": 0, "max": 16777215, "precision": 1 } } },
				{ "type": "devices.capabilities.color_setting", "instance": "colorTemperatureK",
				  "parameters": { "dataType": "INTEGER", "range": { "min": 2700, "max": 6500, "precision": 1 } } },
				{ "type": "devices.capabilities.segment_color_setting", "instance": "segmentedColorRgb",
				  "parameters": { "dataType": "STRUCT", "fields": [
					{ "fieldName": "segment", "dataType": "Array", "elementRange": { "min": 0, "max": 14 }, "elementType": "INTEGER" },
					{ "fieldName": "rgb", "dataType": "INTEGER", "range": { "min": 0, "max": 16777215 } } ] } },
				{ "type": "devices.capabilities.dynamic_scene", "instance": "lightScene",
				  "parameters": { "dataType": "ENUM", "options": [] } },
				{ "type": "devices.capabilities.dynamic_scene", "instance": "snapshot",
				  "parameters": { "dataType": "ENUM", "options": [ { "name": "Evening", "value": 123 } ] } },
				{ "type": "devices.capabilities.music_setting", "instance": "musicMode",
				  "parameters": { "dataType": "STRUCT", "fields": [
					{ "fieldName": "musicMode", "dataType": "ENUM", "options": [ { "name": "Energic", "value": 5 }, { "name": "Rhythm", "value": 3 } ] },
					{ "fieldName": "sensitivity", "dataType": "INTEGER", "range": { "min": 0, "max": 100 } } ] } },
				{ "type": "devices.capabilities.toggle", "instance": "gradientToggle",
				  "parameters": { "dataType": "ENUM", "options": [ { "name": "on", "value": 1 }, { "name": "off", "value": 0 } ] } }
			]
		}))
		.unwrap()
	}

	#[test]
	fn parses_device_features() {
		let device = sample_device();
		assert_eq!(device.display_name(), "Desk lamp");
		let f = device.features();
		assert!(f.power && f.brightness && f.rgb && f.kelvin && f.light);
		assert_eq!((f.kelvin_min, f.kelvin_max), (2700, 6500));
		assert!(f.scene && f.light_scenes && f.snapshots && !f.diy_scenes);
		assert!(f.music && f.toggle && !f.sensor && !f.appliance);
		assert_eq!(f.segments, 15);
		assert_eq!(device.toggles(), vec!["gradientToggle"]);
		assert_eq!(device.music_modes()[0], NamedValue { name: "Energic".into(), value: json!(5) });
		assert_eq!(device.snapshots()[0].name, "Evening");
	}

	#[test]
	fn missing_capabilities_and_defaults() {
		let device: Device =
			serde_json::from_value(json!({ "sku": "H5179", "device": "AA", "capabilities": [] })).unwrap();
		assert_eq!(device.display_name(), "H5179");
		let f = device.features();
		assert!(!f.power && !f.rgb && !f.kelvin && !f.light && !f.scene);
		assert_eq!(f.segments, 0);
		assert_eq!((f.kelvin_min, f.kelvin_max), DEFAULT_KELVIN_RANGE);
	}

	#[test]
	fn appliance_and_sensor_features() {
		let device: Device = serde_json::from_value(json!({
			"sku": "H7131", "device": "BB", "capabilities": [
				{ "type": "devices.capabilities.work_mode", "instance": "workMode", "parameters": { "dataType": "STRUCT", "fields": [] } },
				{ "type": "devices.capabilities.temperature_setting", "instance": "targetTemperature",
				  "parameters": { "dataType": "STRUCT", "fields": [
					{ "fieldName": "temperature", "dataType": "INTEGER", "range": { "min": 5, "max": 30 } } ] } },
				{ "type": "devices.capabilities.property", "instance": "sensorTemperature", "parameters": { "unit": "unit.temperature.celsius" } }
			]
		}))
		.unwrap();
		let f = device.features();
		assert!(f.appliance && f.work_mode && f.target_temperature && f.sensor);
		assert_eq!(device.target_temperature_range(), Some((5, 30)));
		assert!(device.sensor_celsius());
	}

	#[test]
	fn parses_state_response_values() {
		let caps: Vec<StateCapability> = serde_json::from_value(json!([
			{ "type": "devices.capabilities.online", "instance": "online", "state": { "value": false } },
			{ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "state": { "value": 1 } },
			{ "type": "devices.capabilities.range", "instance": "brightness", "state": { "value": 5 } },
			{ "type": "devices.capabilities.color_setting", "instance": "colorRgb", "state": { "value": 16777215 } },
			{ "type": "devices.capabilities.color_setting", "instance": "colorTemperatureK", "state": { "value": 0 } },
			{ "type": "devices.capabilities.toggle", "instance": "gradientToggle", "state": { "value": "" } },
			{ "type": "devices.capabilities.toggle", "instance": "nightlightToggle", "state": { "value": 1 } },
			{ "type": "devices.capabilities.property", "instance": "sensorTemperature", "state": { "value": 71.6 } },
			{ "type": "devices.capabilities.property", "instance": "sensorHumidity", "state": { "value": { "currentHumidity": 44 } } },
			{ "type": "devices.capabilities.work_mode", "instance": "workMode", "state": { "value": { "workMode": 1, "modeValue": 2 } } },
			{ "type": "devices.capabilities.temperature_setting", "instance": "targetTemperature", "state": { "value": { "temperature": 22, "unit": "Celsius" } } }
		]))
		.unwrap();
		let state = DeviceState::from_capabilities(&caps);
		assert_eq!(
			state,
			DeviceState {
				online: Some(false),
				power: Some(true),
				brightness: Some(5),
				rgb: Some(0xffffff),
				kelvin: None,
				toggles: BTreeMap::from([("nightlightToggle".to_owned(), true)]),
				sensor_temperature: Some(71.6),
				sensor_humidity: Some(44.0),
				work_mode: Some((1, 2)),
				target_temperature: Some(22.0),
			}
		);
	}
}
