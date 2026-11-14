use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Targets, action_uuid};
use crate::{render as img, state};

const FALLBACK_COLOR: u32 = 0xff8800;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorSettings {
	pub devices: Vec<DeviceRef>,
	/// `#rrggbb`
	pub color: String,
	/// Hue degrees per dial tick.
	pub dial_step: i32,
}

impl Default for ColorSettings {
	fn default() -> Self {
		Self { devices: Vec::new(), color: color::to_hex(FALLBACK_COLOR), dial_step: 10 }
	}
}

impl ColorSettings {
	fn rgb(&self) -> u32 {
		color::parse_hex(&self.color).unwrap_or(FALLBACK_COLOR)
	}
}

impl Targets for ColorSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// The colour `ticks × step` hue degrees away from `rgb`, at full saturation.
pub fn rotate_hue(rgb: u32, ticks: i16, step: i32) -> u32 {
	color::hue_to_rgb(color::rgb_to_hue(rgb) + ticks as f64 * step as f64)
}

pub struct ColorAction;

#[async_trait]
impl Action for ColorAction {
	const UUID: &'static str = action_uuid!("color");
	type Settings = ColorSettings;

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
		super::dispatch(instance, &settings.devices, Command::Rgb(settings.rgb()), None).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let current = state::primary_state(&settings.devices).rgb.unwrap_or_else(|| settings.rgb());
		let target = rotate_hue(current, ticks, settings.dial_step.max(1));
		super::dispatch(instance, &settings.devices, Command::Rgb(target), Some(DIAL_DEBOUNCE)).await
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

pub async fn render(instance: &Instance, settings: &ColorSettings) -> OpenActionResult<()> {
	if super::is_encoder(instance) {
		let state = state::primary_state(&settings.devices);
		let rgb = state.rgb.unwrap_or_else(|| settings.rgb());
		let hex = color::to_hex(rgb);
		let hue = color::rgb_to_hue(rgb);
		instance
			.set_feedback(&json!({
				"value": super::dial_value_text(state.power, hex.to_uppercase()),
				"indicator": { "value": (hue / 3.6).round() as u32, "bar_fill_c": hex },
				"icon": img::dial_icon(rgb),
			}))
			.await
	} else {
		// A colour key always shows the colour it applies.
		instance.set_image(Some(img::data_url(&img::bulb_key(settings.rgb(), false))), None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hue_rotation() {
		assert_eq!(rotate_hue(0xff0000, 12, 10), 0x00ff00);
		assert_eq!(rotate_hue(0xff0000, -12, 10), 0x0000ff);
		assert_eq!(rotate_hue(0xff0000, 36, 10), 0xff0000);
	}

	#[test]
	fn invalid_color_falls_back() {
		let s = ColorSettings { color: "nope".into(), ..Default::default() };
		assert_eq!(s.rgb(), FALLBACK_COLOR);
	}
}
