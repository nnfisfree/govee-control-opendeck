use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Hold, Targets, action_uuid};
use crate::state;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerMode {
	#[default]
	Toggle,
	On,
	Off,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PowerSettings {
	pub devices: Vec<DeviceRef>,
	pub mode: PowerMode,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Targets for PowerSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

pub struct PowerAction;

impl PowerAction {
	async fn press(&self, instance: &Instance, settings: &PowerSettings) -> OpenActionResult<()> {
		let on = match settings.mode {
			PowerMode::Toggle => !state::primary_state(&settings.devices).power.unwrap_or(false),
			PowerMode::On => true,
			PowerMode::Off => false,
		};
		super::dispatch(instance, &settings.devices, Command::Power(on), None).await
	}
}

#[async_trait]
impl Action for PowerAction {
	const UUID: &'static str = action_uuid!("power");
	type Settings = PowerSettings;

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

pub async fn render(instance: &Instance, settings: &PowerSettings) -> OpenActionResult<()> {
	let power = state::primary_state(&settings.devices).power;
	if super::is_encoder(instance) {
		let text = match power {
			Some(true) => "On",
			Some(false) => "Off",
			None => "–",
		};
		instance
			.set_feedback(
				&json!({ "value": text, "indicator": { "value": if power == Some(true) { 100 } else { 0 } } }),
			)
			.await
	} else if let Some(on) = power {
		instance.set_state(on as u16).await
	} else {
		Ok(())
	}
}
