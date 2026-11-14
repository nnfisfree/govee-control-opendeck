use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability `type` / `instance` pairs used by this plugin.
pub mod cap {
	pub const ON_OFF: &str = "devices.capabilities.on_off";
	pub const RANGE: &str = "devices.capabilities.range";
	pub const COLOR_SETTING: &str = "devices.capabilities.color_setting";
	pub const ONLINE: &str = "devices.capabilities.online";

	pub const POWER_SWITCH: &str = "powerSwitch";
	pub const BRIGHTNESS: &str = "brightness";
	pub const COLOR_RGB: &str = "colorRgb";
	pub const COLOR_TEMPERATURE_K: &str = "colorTemperatureK";
	pub const ONLINE_INSTANCE: &str = "online";
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

	pub fn features(&self) -> Features {
		let kelvin = self.capability(cap::COLOR_SETTING, cap::COLOR_TEMPERATURE_K);
		let (kelvin_min, kelvin_max) =
			kelvin.and_then(|c| c.range()).unwrap_or((DEFAULT_KELVIN_RANGE.0 as i64, DEFAULT_KELVIN_RANGE.1 as i64));
		Features {
			power: self.capability(cap::ON_OFF, cap::POWER_SWITCH).is_some(),
			brightness: self.capability(cap::RANGE, cap::BRIGHTNESS).is_some(),
			rgb: self.capability(cap::COLOR_SETTING, cap::COLOR_RGB).is_some(),
			kelvin: kelvin.is_some(),
			kelvin_min: kelvin_min.max(0) as u32,
			kelvin_max: kelvin_max.max(0) as u32,
		}
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
}

/// Last known state of one device (from `/device/state` or optimistic updates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceState {
	pub online: Option<bool>,
	pub power: Option<bool>,
	pub brightness: Option<u8>,
	pub rgb: Option<u32>,
	pub kelvin: Option<u32>,
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
			"device": "9D:FA:85:EB:D3:00:8B:FF",
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
				  "parameters": { "dataType": "INTEGER", "range": { "min": 2700, "max": 6500, "precision": 1 } } }
			]
		}))
		.unwrap()
	}

	#[test]
	fn parses_device_features() {
		let device = sample_device();
		assert_eq!(device.display_name(), "Desk lamp");
		let f = device.features();
		assert!(f.power && f.brightness && f.rgb && f.kelvin);
		assert_eq!((f.kelvin_min, f.kelvin_max), (2700, 6500));
	}

	#[test]
	fn missing_capabilities_and_defaults() {
		let device: Device =
			serde_json::from_value(json!({ "sku": "H5179", "device": "AA", "capabilities": [] })).unwrap();
		assert_eq!(device.display_name(), "H5179");
		let f = device.features();
		assert!(!f.power && !f.rgb && !f.kelvin);
		assert_eq!((f.kelvin_min, f.kelvin_max), DEFAULT_KELVIN_RANGE);
	}

	#[test]
	fn parses_state_response_values() {
		let caps: Vec<StateCapability> = serde_json::from_value(json!([
			{ "type": "devices.capabilities.online", "instance": "online", "state": { "value": false } },
			{ "type": "devices.capabilities.on_off", "instance": "powerSwitch", "state": { "value": 1 } },
			{ "type": "devices.capabilities.range", "instance": "brightness", "state": { "value": 5 } },
			{ "type": "devices.capabilities.color_setting", "instance": "colorRgb", "state": { "value": 16777215 } },
			{ "type": "devices.capabilities.color_setting", "instance": "colorTemperatureK", "state": { "value": 0 } },
			{ "type": "devices.capabilities.toggle", "instance": "gradientToggle", "state": { "value": "" } }
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
			}
		);
	}
}
