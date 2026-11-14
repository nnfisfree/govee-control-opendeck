use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Mode, Targets, action_uuid};
use crate::state;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrightnessSettings {
	pub devices: Vec<DeviceRef>,
	/// Key: set a fixed value or step by `step`.
	pub mode: Mode,
	pub value: u8,
	pub step: i32,
	/// Percent per dial tick.
	pub dial_step: i32,
	/// Show the current brightness as the key title.
	pub show_value: bool,
}

impl Default for BrightnessSettings {
	fn default() -> Self {
		Self { devices: Vec::new(), mode: Mode::Set, value: 50, step: 10, dial_step: 5, show_value: true }
	}
}

impl Targets for BrightnessSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// Brightness `current` moved by `delta`, clamped to Govee's 1..=100.
pub fn step_brightness(current: u8, delta: i32) -> u8 {
	(current as i32 + delta).clamp(1, 100) as u8
}

fn current(settings: &BrightnessSettings) -> u8 {
	state::primary_state(&settings.devices).brightness.unwrap_or(settings.value)
}

pub struct BrightnessAction;

#[async_trait]
impl Action for BrightnessAction {
	const UUID: &'static str = action_uuid!("brightness");
	type Settings = BrightnessSettings;

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
		let target = match settings.mode {
			Mode::Set => settings.value.clamp(1, 100),
			Mode::Step => step_brightness(current(settings), settings.step),
		};
		super::dispatch(instance, &settings.devices, Command::Brightness(target), None).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let target = step_brightness(current(settings), ticks as i32 * settings.dial_step.max(1));
		super::dispatch(instance, &settings.devices, Command::Brightness(target), Some(DIAL_DEBOUNCE)).await
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

pub async fn render(instance: &Instance, settings: &BrightnessSettings) -> OpenActionResult<()> {
	let state = state::primary_state(&settings.devices);
	if super::is_encoder(instance) {
		let value = state.brightness.unwrap_or(0);
		let text = state.brightness.map(|b| format!("{b}%")).unwrap_or_else(|| "–".into());
		instance
			.set_feedback(&json!({
				"value": super::dial_value_text(state.power, text),
				"indicator": { "value": value },
			}))
			.await
	} else if settings.show_value {
		let title = state.brightness.map(|b| format!("{b}%"));
		instance.set_title(title, None).await
	} else {
		instance.set_title(None::<String>, None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn brightness_steps_clamp() {
		assert_eq!(step_brightness(50, 10), 60);
		assert_eq!(step_brightness(95, 10), 100);
		assert_eq!(step_brightness(5, -10), 1);
		assert_eq!(step_brightness(1, -1), 1);
	}

	#[test]
	fn settings_defaults_fill_missing_fields() {
		let s: BrightnessSettings = serde_json::from_value(json!({ "mode": "step" })).unwrap();
		assert_eq!(s.mode, Mode::Step);
		assert_eq!((s.value, s.step, s.dial_step), (50, 10, 5));
		assert!(s.show_value);
	}
}
