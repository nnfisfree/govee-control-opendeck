//! Toggle: switch a device feature on or off, such as the gradient, the night
//! light, DreamView (TV sync) or a fan's oscillation.

use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::power::PowerMode;
use super::{Hold, Targets, action_uuid};
use crate::state;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToggleSettings {
	pub devices: Vec<DeviceRef>,
	/// The capability instance, e.g. `gradientToggle`.
	pub instance: String,
	pub mode: PowerMode,
	pub show_name: bool,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for ToggleSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			instance: String::new(),
			mode: PowerMode::Toggle,
			show_name: true,
			hold: Hold::default(),
		}
	}
}

impl Targets for ToggleSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// "dreamViewToggle" → "Dream View".
pub fn friendly_name(instance: &str) -> String {
	let base = instance.strip_suffix("Toggle").unwrap_or(instance);
	let mut out = String::new();
	for (i, c) in base.chars().enumerate() {
		if i == 0 {
			out.extend(c.to_uppercase());
		} else if c.is_uppercase() {
			out.push(' ');
			out.push(c);
		} else {
			out.push(c);
		}
	}
	out
}

fn current(settings: &ToggleSettings) -> Option<bool> {
	state::primary_state(&settings.devices).toggles.get(&settings.instance).copied()
}

pub struct ToggleAction;

impl ToggleAction {
	async fn press(&self, instance: &Instance, settings: &ToggleSettings) -> OpenActionResult<()> {
		if settings.instance.is_empty() {
			log::warn!("Toggle key has no feature selected");
			return instance.show_alert().await;
		}
		let on = match settings.mode {
			PowerMode::Toggle => !current(settings).unwrap_or(false),
			PowerMode::On => true,
			PowerMode::Off => false,
		};
		let command = Command::Toggle { instance: settings.instance.clone(), on };
		super::dispatch(instance, &settings.devices, command, None).await
	}
}

#[async_trait]
impl Action for ToggleAction {
	const UUID: &'static str = action_uuid!("toggle");
	type Settings = ToggleSettings;

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
		self.press(instance, settings).await
	}
}

pub async fn render(instance: &Instance, settings: &ToggleSettings) -> OpenActionResult<()> {
	let on = current(settings);
	let name = if settings.instance.is_empty() { "Toggle".to_owned() } else { friendly_name(&settings.instance) };
	if super::is_encoder(instance) {
		let text = match on {
			Some(true) => "On",
			Some(false) => "Off",
			None => "–",
		};
		return instance.set_feedback(&json!({ "title": name, "value": text })).await;
	}
	if let Some(on) = on {
		instance.set_state(on as u16).await?;
	}
	let title = settings.show_name.then(|| super::wrap_title(&name));
	instance.set_title(title, None).await
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn friendly_names() {
		assert_eq!(friendly_name("gradientToggle"), "Gradient");
		assert_eq!(friendly_name("dreamViewToggle"), "Dream View");
		assert_eq!(friendly_name("nightlightToggle"), "Nightlight");
		assert_eq!(friendly_name("oscillation"), "Oscillation");
	}
}
