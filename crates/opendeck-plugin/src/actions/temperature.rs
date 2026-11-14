use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Mode, Targets, action_uuid};
use crate::{render as img, state};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TemperatureSettings {
	pub devices: Vec<DeviceRef>,
	pub mode: Mode,
	/// Kelvin for "set" mode.
	pub value: u32,
	/// Kelvin per key press in "step" mode (may be negative).
	pub step: i32,
	/// Kelvin per dial tick.
	pub dial_step: i32,
	pub show_value: bool,
}

impl Default for TemperatureSettings {
	fn default() -> Self {
		Self { devices: Vec::new(), mode: Mode::Set, value: 4000, step: 500, dial_step: 100, show_value: true }
	}
}

impl Targets for TemperatureSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// `current` moved by `delta` Kelvin and clamped to `range`.
pub fn step_kelvin(current: u32, delta: i32, range: (u32, u32)) -> u32 {
	(current as i64 + delta as i64).clamp(range.0 as i64, range.1 as i64) as u32
}

/// Position of `kelvin` within `range`, 0..=100, for the dial bar.
pub fn kelvin_percent(kelvin: u32, range: (u32, u32)) -> u32 {
	if range.1 <= range.0 {
		return 0;
	}
	let k = kelvin.clamp(range.0, range.1);
	((k - range.0) as u64 * 100 / (range.1 - range.0) as u64) as u32
}

fn current(settings: &TemperatureSettings, range: (u32, u32)) -> u32 {
	state::primary_state(&settings.devices).kelvin.unwrap_or(settings.value).clamp(range.0, range.1)
}

pub struct TemperatureAction;

#[async_trait]
impl Action for TemperatureAction {
	const UUID: &'static str = action_uuid!("temperature");
	type Settings = TemperatureSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
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
		crate::pi::handle(instance, payload).await
	}

	async fn key_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		let range = state::kelvin_range(&settings.devices);
		let target = match settings.mode {
			Mode::Set => settings.value.clamp(range.0, range.1),
			Mode::Step => step_kelvin(current(settings, range), settings.step, range),
		};
		super::dispatch(instance, &settings.devices, Command::Kelvin(target), None).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let range = state::kelvin_range(&settings.devices);
		let target = step_kelvin(current(settings, range), ticks as i32 * settings.dial_step.max(1), range);
		super::dispatch(instance, &settings.devices, Command::Kelvin(target), Some(DIAL_DEBOUNCE)).await
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

pub async fn render(instance: &Instance, settings: &TemperatureSettings) -> OpenActionResult<()> {
	let state = state::primary_state(&settings.devices);
	let range = state::kelvin_range(&settings.devices);
	if super::is_encoder(instance) {
		let shown = state.kelvin.unwrap_or(settings.value);
		let tint = color::to_hex(color::kelvin_to_rgb(shown));
		let text = state.kelvin.map(|k| format!("{k}K")).unwrap_or_else(|| "–".into());
		instance
			.set_feedback(&json!({
				"value": super::dial_value_text(state.power, text),
				"indicator": { "value": kelvin_percent(shown, range), "bar_fill_c": tint },
				"icon": img::dial_icon(color::kelvin_to_rgb(shown)),
			}))
			.await
	} else {
		// A "set" key shows the colour it will apply; a "step" key shows the current one.
		let shown = match settings.mode {
			Mode::Set => settings.value,
			Mode::Step => state.kelvin.unwrap_or(settings.value),
		};
		instance.set_image(Some(img::data_url(&img::bulb_key(color::kelvin_to_rgb(shown), false))), None).await?;
		let title = if settings.show_value {
			match settings.mode {
				Mode::Set => Some(format!("{}K", settings.value)),
				Mode::Step => state.kelvin.map(|k| format!("{k}K")),
			}
		} else {
			None
		};
		instance.set_title(title, None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn kelvin_math() {
		let range = (2000, 9000);
		assert_eq!(step_kelvin(4000, 500, range), 4500);
		assert_eq!(step_kelvin(8800, 500, range), 9000);
		assert_eq!(step_kelvin(2100, -500, range), 2000);
		assert_eq!(kelvin_percent(2000, range), 0);
		assert_eq!(kelvin_percent(5500, range), 50);
		assert_eq!(kelvin_percent(12000, range), 100);
		assert_eq!(kelvin_percent(4000, (5000, 5000)), 0);
	}
}
