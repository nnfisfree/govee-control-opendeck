//! Music mode: make lights react to sound with a chosen style and sensitivity.
//!
//! Music mode values differ between models, so the settings store the mode
//! *name* and each light looks up its own value.

use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Hold, Targets, action_uuid};
use crate::state::{self, Jobs};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MusicSettings {
	pub devices: Vec<DeviceRef>,
	/// Music mode name, e.g. "Energic". Empty means the light's first mode.
	pub mode: String,
	/// 0..=100
	pub sensitivity: u8,
	pub auto_color: bool,
	/// `#rrggbb`, used when `auto_color` is off.
	pub color: String,
	/// Sensitivity per dial tick.
	pub dial_step: i32,
	pub show_name: bool,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for MusicSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			mode: String::new(),
			sensitivity: 50,
			auto_color: true,
			color: "#ff0000".into(),
			dial_step: 5,
			show_name: true,
			hold: Hold::default(),
		}
	}
}

impl Targets for MusicSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// The music mode command for every light that has the chosen mode.
fn music_jobs(settings: &MusicSettings, sensitivity: u8) -> Jobs {
	let rgb = color::parse_hex(&settings.color).unwrap_or(0xff0000);
	let mut jobs = Vec::new();
	for target in &settings.devices {
		let modes = state::device(target).map(|d| d.music_modes()).unwrap_or_default();
		let found = if settings.mode.trim().is_empty() {
			modes.first()
		} else {
			modes.iter().find(|m| m.name.eq_ignore_ascii_case(settings.mode.trim()))
		};
		match found.and_then(|m| m.value.as_i64()) {
			Some(mode) => jobs.push((
				target.clone(),
				vec![Command::Music { mode, sensitivity, auto_color: settings.auto_color, rgb }],
			)),
			None => log::warn!("Music mode '{}' is not available on {}", settings.mode, target.device),
		}
	}
	jobs
}

/// Sensitivity the dial is on, or the configured one.
fn sensitivity(instance: &Instance, settings: &MusicSettings) -> u8 {
	state::cursor(&instance.instance_id).map(|s| s.min(100) as u8).unwrap_or(settings.sensitivity.min(100))
}

pub struct MusicAction;

impl MusicAction {
	async fn press(&self, instance: &Instance, settings: &MusicSettings) -> OpenActionResult<()> {
		super::dispatch_jobs(instance, music_jobs(settings, settings.sensitivity.min(100)), None).await
	}
}

#[async_trait]
impl Action for MusicAction {
	const UUID: &'static str = action_uuid!("music");
	type Settings = MusicSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		// Music modes come from the device list.
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

	/// Turning the dial changes the sensitivity (and starts music mode).
	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let current = sensitivity(instance, settings) as i32;
		let next = (current + ticks as i32 * settings.dial_step.max(1)).clamp(0, 100) as u8;
		state::set_cursor(&instance.instance_id, next as usize);
		super::dispatch_jobs(instance, music_jobs(settings, next), Some(DIAL_DEBOUNCE)).await
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

pub async fn render(instance: &Instance, settings: &MusicSettings) -> OpenActionResult<()> {
	let name = if settings.mode.trim().is_empty() { "Music".to_owned() } else { settings.mode.clone() };
	if super::is_encoder(instance) {
		let power = state::primary_state(&settings.devices).power;
		let level = sensitivity(instance, settings);
		instance
			.set_feedback(&json!({
				"title": name,
				"value": super::dial_value_text(power, format!("{level}%")),
				"indicator": { "value": level },
			}))
			.await
	} else if settings.show_name {
		instance.set_title(Some(super::wrap_title(&name)), None).await
	} else {
		instance.set_title(None::<String>, None).await
	}
}
