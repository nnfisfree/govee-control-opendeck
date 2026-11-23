//! Preset: one press sets power, colour (or white temperature) and brightness
//! together, e.g. "Movie: on, warm orange, 20%".

use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::brightness::step_brightness;
use super::{DIAL_DEBOUNCE, Hold, Targets, action_uuid};
use crate::{render as img, state};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetColor {
	/// Leave the colour as it is.
	None,
	#[default]
	Rgb,
	Kelvin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PresetSettings {
	pub devices: Vec<DeviceRef>,
	pub turn_on: bool,
	pub set_brightness: bool,
	pub brightness: u8,
	pub color_mode: PresetColor,
	/// `#rrggbb`
	pub color: String,
	pub kelvin: u32,
	/// Brightness percent per dial tick.
	pub dial_step: i32,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for PresetSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			turn_on: true,
			set_brightness: true,
			brightness: 80,
			color_mode: PresetColor::Rgb,
			color: "#ffb347".into(),
			kelvin: 3000,
			dial_step: 5,
			hold: Hold::default(),
		}
	}
}

impl Targets for PresetSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

impl PresetSettings {
	/// Commands in the order they are sent: power first, so the light is on
	/// before the colour and brightness change.
	pub fn commands(&self, kelvin_range: (u32, u32)) -> Vec<Command> {
		let mut commands = Vec::new();
		if self.turn_on {
			commands.push(Command::Power(true));
		}
		match self.color_mode {
			PresetColor::None => {}
			PresetColor::Rgb => commands.push(Command::Rgb(color::parse_hex(&self.color).unwrap_or(0xffb347))),
			PresetColor::Kelvin => commands.push(Command::Kelvin(self.kelvin.clamp(kelvin_range.0, kelvin_range.1))),
		}
		if self.set_brightness {
			commands.push(Command::Brightness(self.brightness.clamp(1, 100)));
		}
		commands
	}

	/// The colour drawn on the key.
	fn display_rgb(&self) -> u32 {
		match self.color_mode {
			PresetColor::None => 0xffffff,
			PresetColor::Rgb => color::parse_hex(&self.color).unwrap_or(0xffb347),
			PresetColor::Kelvin => color::kelvin_to_rgb(self.kelvin),
		}
	}
}

pub struct PresetAction;

impl PresetAction {
	async fn press(&self, instance: &Instance, settings: &PresetSettings) -> OpenActionResult<()> {
		let commands = settings.commands(state::kelvin_range(&settings.devices));
		super::dispatch_jobs(instance, state::jobs_for(&settings.devices, &commands), None).await
	}
}

#[async_trait]
impl Action for PresetAction {
	const UUID: &'static str = action_uuid!("preset");
	type Settings = PresetSettings;

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

	/// Turning the dial fine-tunes the brightness after applying the preset.
	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let current = state::primary_state(&settings.devices).brightness.unwrap_or(settings.brightness);
		let target = step_brightness(current, ticks as i32 * settings.dial_step.max(1));
		super::dispatch(instance, &settings.devices, Command::Brightness(target), Some(DIAL_DEBOUNCE)).await
	}

	async fn dial_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		self.press(instance, settings).await
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

pub async fn render(instance: &Instance, settings: &PresetSettings) -> OpenActionResult<()> {
	let rgb = settings.display_rgb();
	if super::is_encoder(instance) {
		let state = state::primary_state(&settings.devices);
		let text = state.brightness.map(|b| format!("{b}%")).unwrap_or_else(|| "–".into());
		instance
			.set_feedback(&json!({
				"title": "Preset",
				"value": super::dial_value_text(state.power, text),
				"indicator": { "value": state.brightness.unwrap_or(0) },
				"icon": img::dial_icon(rgb),
			}))
			.await
	} else {
		instance.set_image(Some(img::data_url(&img::bulb_key(rgb, false))), None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn preset_commands_in_order() {
		let s = PresetSettings::default();
		assert_eq!(
			s.commands((2000, 9000)),
			vec![Command::Power(true), Command::Rgb(0xffb347), Command::Brightness(80)]
		);
		let s = PresetSettings {
			turn_on: false,
			set_brightness: false,
			color_mode: PresetColor::Kelvin,
			kelvin: 9500,
			..Default::default()
		};
		assert_eq!(s.commands((2700, 6500)), vec![Command::Kelvin(6500)]);
		let s = PresetSettings { color_mode: PresetColor::None, brightness: 0, ..Default::default() };
		assert_eq!(s.commands((2000, 9000)), vec![Command::Power(true), Command::Brightness(1)]);
	}
}
