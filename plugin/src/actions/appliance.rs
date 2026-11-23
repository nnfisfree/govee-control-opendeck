//! Appliance: set the work mode of a Govee heater, fan, purifier or
//! humidifier (e.g. "Gear mode: High"), or its target temperature.
//! Use the Power action to switch appliances on and off.

use govee_api::{Command, Device, DeviceRef, NamedValue, cap, options_of};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{DIAL_DEBOUNCE, Hold, Mode, Targets, action_uuid};
use crate::state;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplianceFunction {
	#[default]
	Mode,
	Temperature,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApplianceSettings {
	pub devices: Vec<DeviceRef>,
	pub function: ApplianceFunction,
	/// `workMode` value, from the device's options.
	pub work_mode: i64,
	/// `modeValue` value: a level, speed or humidity, depending on the mode.
	pub mode_value: i64,
	/// Shown on the key, e.g. "Gear: High". Filled in by the settings panel.
	pub mode_label: String,
	pub temperature: i64,
	pub fahrenheit: bool,
	/// Set a fixed temperature or step it.
	pub temperature_mode: Mode,
	pub temperature_step: i64,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for ApplianceSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			function: ApplianceFunction::Mode,
			work_mode: 1,
			mode_value: 0,
			mode_label: String::new(),
			temperature: 22,
			fahrenheit: false,
			temperature_mode: Mode::Set,
			temperature_step: 1,
			hold: Hold::default(),
		}
	}
}

impl Targets for ApplianceSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// The values a work mode accepts.
#[derive(Clone, Debug, PartialEq)]
pub enum ModeValues {
	List(Vec<NamedValue>),
	Range(i64, i64),
	Fixed(i64),
}

/// Look up which values `work_mode` accepts on `device`. Govee lists the
/// modes in the `workMode` field and, for each mode name, its values in the
/// `modeValue` field.
pub fn mode_values(device: &Device, work_mode: i64) -> ModeValues {
	let Some(c) = device.capability(cap::WORK_MODE, cap::WORK_MODE_INSTANCE) else { return ModeValues::Fixed(0) };
	let name = c
		.field("workMode")
		.map(options_of)
		.unwrap_or_default()
		.into_iter()
		.find(|o| o.value.as_i64() == Some(work_mode))
		.map(|o| o.name);
	let entry =
		c.field("modeValue").and_then(|f| f.get("options")).and_then(Value::as_array).and_then(|options| {
			options.iter().find(|o| o.get("name").and_then(Value::as_str) == name.as_deref()).cloned()
		});
	let Some(entry) = entry else { return ModeValues::Fixed(0) };
	let list = options_of(&entry);
	if !list.is_empty() {
		return ModeValues::List(list);
	}
	if let Some(range) = entry.get("range")
		&& let (Some(min), Some(max)) =
			(range.get("min").and_then(Value::as_i64), range.get("max").and_then(Value::as_i64))
	{
		return ModeValues::Range(min, max);
	}
	ModeValues::Fixed(entry.get("defaultValue").and_then(Value::as_i64).unwrap_or(0))
}

/// Next mode value `ticks` steps from `current`.
pub fn step_mode_value(values: &ModeValues, current: i64, ticks: i64) -> i64 {
	match values {
		ModeValues::List(list) if !list.is_empty() => {
			let position = list.iter().position(|o| o.value.as_i64() == Some(current)).unwrap_or(0);
			let next = (position as i64 + ticks).clamp(0, list.len() as i64 - 1) as usize;
			list[next].value.as_i64().unwrap_or(current)
		}
		ModeValues::Range(min, max) => (current + ticks).clamp(*min, *max),
		ModeValues::List(_) => current,
		ModeValues::Fixed(value) => *value,
	}
}

fn temperature_range(settings: &ApplianceSettings) -> (i64, i64) {
	settings
		.devices
		.first()
		.and_then(state::device)
		.and_then(|d| d.target_temperature_range())
		.unwrap_or(if settings.fahrenheit { (40, 95) } else { (5, 35) })
}

fn current_temperature(settings: &ApplianceSettings) -> i64 {
	state::primary_state(&settings.devices).target_temperature.map(|t| t.round() as i64).unwrap_or(settings.temperature)
}

fn temperature_command(settings: &ApplianceSettings, temperature: i64) -> Command {
	let (min, max) = temperature_range(settings);
	Command::TargetTemperature { temperature: temperature.clamp(min, max), fahrenheit: settings.fahrenheit }
}

pub struct ApplianceAction;

impl ApplianceAction {
	async fn press(&self, instance: &Instance, settings: &ApplianceSettings) -> OpenActionResult<()> {
		let command = match settings.function {
			ApplianceFunction::Mode => Command::WorkMode { mode: settings.work_mode, value: settings.mode_value },
			ApplianceFunction::Temperature => {
				let target = match settings.temperature_mode {
					Mode::Set => settings.temperature,
					Mode::Step => current_temperature(settings) + settings.temperature_step,
				};
				temperature_command(settings, target)
			}
		};
		super::dispatch(instance, &settings.devices, command, None).await
	}
}

#[async_trait]
impl Action for ApplianceAction {
	const UUID: &'static str = action_uuid!("appliance");
	type Settings = ApplianceSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		tokio::spawn(state::devices_or_fetch());
		super::appeared(instance, settings).await
	}

	async fn did_receive_settings(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		super::appeared(instance, settings).await
	}

	async fn will_disappear(&self, instance: &Instance, _settings: &Self::Settings) -> OpenActionResult<()> {
		super::disappeared(instance);
		Ok(())
	}

	async fn send_to_plugin(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
		payload: &serde_json::Value,
	) -> OpenActionResult<()> {
		crate::pi::handle(instance, payload)
	}

	async fn key_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		if super::key_down(instance, &settings.devices, &settings.hold) {
			self.press(instance, settings).await?;
		}
		Ok(())
	}

	async fn key_up(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		if super::key_up(instance, &settings.hold) {
			self.press(instance, settings).await?;
		}
		Ok(())
	}

	/// Turning the dial changes the target temperature, or steps through the
	/// chosen mode's levels.
	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let command = match settings.function {
			ApplianceFunction::Temperature => {
				temperature_command(settings, current_temperature(settings) + ticks as i64)
			}
			ApplianceFunction::Mode => {
				let Some(device) = settings.devices.first().and_then(state::device) else {
					return instance.show_alert().await;
				};
				let current = match state::primary_state(&settings.devices).work_mode {
					Some((mode, value)) if mode == settings.work_mode => value,
					_ => settings.mode_value,
				};
				let value = step_mode_value(&mode_values(&device, settings.work_mode), current, ticks as i64);
				Command::WorkMode { mode: settings.work_mode, value }
			}
		};
		super::dispatch(instance, &settings.devices, command, Some(DIAL_DEBOUNCE)).await
	}

	async fn dial_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		super::toggle_power(instance, &settings.devices).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		super::toggle_power(instance, &settings.devices).await
	}
}

pub async fn render(instance: &Instance, settings: &ApplianceSettings) -> OpenActionResult<()> {
	let state = state::primary_state(&settings.devices);
	let unit = if settings.fahrenheit { "°F" } else { "°C" };
	if super::is_encoder(instance) {
		let (title, text, indicator) = match settings.function {
			ApplianceFunction::Temperature => {
				let t = current_temperature(settings);
				let (min, max) = temperature_range(settings);
				let percent = if max > min { ((t - min) * 100 / (max - min)).clamp(0, 100) } else { 0 };
				("Target".to_owned(), format!("{t}{unit}"), percent)
			}
			ApplianceFunction::Mode => {
				let value = state.work_mode.filter(|(m, _)| *m == settings.work_mode).map(|(_, v)| v);
				let device = settings.devices.first().and_then(state::device);
				let (text, percent) = match (device.map(|d| mode_values(&d, settings.work_mode)), value) {
					(Some(ModeValues::List(list)), Some(v)) => {
						let position = list.iter().position(|o| o.value.as_i64() == Some(v));
						let name = position.map(|p| list[p].name.clone()).unwrap_or_else(|| v.to_string());
						let percent = position.map(|p| (p as i64 + 1) * 100 / list.len() as i64).unwrap_or(0);
						(name, percent)
					}
					(Some(ModeValues::Range(min, max)), Some(v)) if max > min => {
						(v.to_string(), ((v - min) * 100 / (max - min)).clamp(0, 100))
					}
					(_, Some(v)) => (v.to_string(), 0),
					(_, None) => ("–".to_owned(), 0),
				};
				let title =
					if settings.mode_label.is_empty() { "Mode".to_owned() } else { settings.mode_label.clone() };
				(title, text, percent)
			}
		};
		return instance
			.set_feedback(&json!({
				"title": title,
				"value": super::dial_value_text(state.power, text),
				"indicator": { "value": indicator },
			}))
			.await;
	}
	let title = match settings.function {
		ApplianceFunction::Mode if !settings.mode_label.is_empty() => Some(super::wrap_title(&settings.mode_label)),
		ApplianceFunction::Mode => None,
		ApplianceFunction::Temperature => Some(match settings.temperature_mode {
			Mode::Set => format!("{}{unit}", settings.temperature),
			Mode::Step => format!("{}{unit}", current_temperature(settings)),
		}),
	};
	instance.set_title(title, None).await
}

#[cfg(test)]
mod tests {
	use super::*;

	fn fan() -> Device {
		serde_json::from_value(json!({
			"sku": "H7102", "device": "CC", "capabilities": [{
				"type": "devices.capabilities.work_mode", "instance": "workMode",
				"parameters": { "dataType": "STRUCT", "fields": [
					{ "fieldName": "workMode", "dataType": "ENUM", "options": [
						{ "name": "gearMode", "value": 1 }, { "name": "Fan", "value": 9 }, { "name": "Auto", "value": 3 } ] },
					{ "fieldName": "modeValue", "dataType": "ENUM", "options": [
						{ "name": "gearMode", "options": [ { "name": "Low", "value": 1 }, { "name": "Medium", "value": 2 }, { "name": "High", "value": 3 } ] },
						{ "name": "Fan", "defaultValue": 0 },
						{ "name": "Auto", "range": { "min": 40, "max": 80 } } ] } ] }
			}]
		}))
		.unwrap()
	}

	#[test]
	fn mode_values_from_capabilities() {
		let device = fan();
		match mode_values(&device, 1) {
			ModeValues::List(list) => {
				assert_eq!(list.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["Low", "Medium", "High"])
			}
			other => panic!("unexpected {other:?}"),
		}
		assert_eq!(mode_values(&device, 9), ModeValues::Fixed(0));
		assert_eq!(mode_values(&device, 3), ModeValues::Range(40, 80));
		assert_eq!(mode_values(&device, 42), ModeValues::Fixed(0));
	}

	#[test]
	fn stepping_mode_values() {
		let device = fan();
		let gears = mode_values(&device, 1);
		assert_eq!(step_mode_value(&gears, 1, 1), 2);
		assert_eq!(step_mode_value(&gears, 3, 1), 3);
		assert_eq!(step_mode_value(&gears, 2, -5), 1);
		assert_eq!(step_mode_value(&ModeValues::Range(40, 80), 78, 5), 80);
		assert_eq!(step_mode_value(&ModeValues::Fixed(0), 5, 1), 0);
	}
}
