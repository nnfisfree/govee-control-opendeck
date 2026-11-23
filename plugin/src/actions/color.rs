use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Hold, Targets, action_uuid};
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
	/// What turning the dial does.
	pub dial_mode: DialMode,
	/// Colours the dial steps through in list mode (`#rrggbb`, comma separated).
	pub dial_colors: String,
	#[serde(flatten)]
	pub hold: Hold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DialMode {
	/// Shift the hue continuously.
	#[default]
	Hue,
	/// Step through a fixed list of colours.
	List,
}

impl Default for ColorSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			color: color::to_hex(FALLBACK_COLOR),
			dial_step: 10,
			dial_mode: DialMode::Hue,
			dial_colors: DEFAULT_DIAL_COLORS.into(),
			hold: Hold::default(),
		}
	}
}

const DEFAULT_DIAL_COLORS: &str = "#ff0000, #ff8800, #ffd60a, #30d158, #00c7be, #0a84ff, #5e5ce6, #ff2d92, #ffffff";

/// Parse a list of `#rrggbb` colours separated by commas, spaces or new lines.
/// Invalid entries are skipped.
pub fn parse_colors(list: &str) -> Vec<u32> {
	list.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter_map(color::parse_hex).collect()
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

impl ColorAction {
	async fn press(&self, instance: &Instance, settings: &ColorSettings) -> OpenActionResult<()> {
		super::dispatch(instance, &settings.devices, Command::Rgb(settings.rgb()), None).await
	}
}

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

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let current = state::primary_state(&settings.devices).rgb.unwrap_or_else(|| settings.rgb());
		let target = match settings.dial_mode {
			DialMode::Hue => rotate_hue(current, ticks, settings.dial_step.max(1)),
			DialMode::List => {
				let colors = parse_colors(&settings.dial_colors);
				if colors.is_empty() {
					return instance.show_alert().await;
				}
				// Start from the colour the light shows, if it is in the list.
				let position = state::cursor(&instance.instance_id)
					.filter(|&i| colors.get(i) == Some(&current))
					.or_else(|| colors.iter().position(|&c| c == current));
				let next = match position {
					Some(i) => super::wrap_index(i, ticks as i64, colors.len()),
					None if ticks > 0 => 0,
					None => colors.len() - 1,
				};
				state::set_cursor(&instance.instance_id, next);
				colors[next]
			}
		};
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
	fn color_lists() {
		assert_eq!(parse_colors("#ff0000, #00ff00\n#0000ff;bad"), vec![0xff0000, 0x00ff00, 0x0000ff]);
		assert_eq!(parse_colors(DEFAULT_DIAL_COLORS).len(), 9);
		assert!(parse_colors("").is_empty());
	}

	#[test]
	fn invalid_color_falls_back() {
		let s = ColorSettings { color: "nope".into(), ..Default::default() };
		assert_eq!(s.rgb(), FALLBACK_COLOR);
	}
}
