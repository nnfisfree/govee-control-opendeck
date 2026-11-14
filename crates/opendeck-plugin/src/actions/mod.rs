//! The four v1 actions and the lifecycle/dispatch code they share.
//!
//! Every action targets a list of devices (`devices` in its settings), so any
//! key or dial can control a single light or a group.

mod brightness;
mod color;
mod power;
mod temperature;

use std::time::Duration;

use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use brightness::BrightnessAction;
pub use color::ColorAction;
pub use power::PowerAction;
pub use temperature::TemperatureAction;

use crate::state;

/// Plugin bundle ID. Must match `assets/manifest.json` and the folder name
/// `<bundle-id>.sdPlugin`. Change it everywhere with `scripts/set-bundle-id.sh`.
macro_rules! action_uuid {
	($name:literal) => {
		concat!("io.github.nnfisfree.govee-control", ".", $name)
	};
}
pub(crate) use action_uuid;

/// Delay between the last dial tick and the API call.
pub const DIAL_DEBOUNCE: Duration = Duration::from_millis(250);

/// Settings that name the devices an instance controls.
pub trait Targets {
	fn targets(&self) -> &[DeviceRef];
}

pub async fn register_all() {
	openaction::register_action(PowerAction).await;
	openaction::register_action(BrightnessAction).await;
	openaction::register_action(ColorAction).await;
	openaction::register_action(TemperatureAction).await;
}

pub fn is_encoder(instance: &Instance) -> bool {
	instance.controller == "Encoder"
}

/// Remember an instance's settings, draw it from the cache straight away, then
/// fetch fresh state in the background and redraw.
pub async fn appeared<S: Serialize + Targets>(instance: &Instance, settings: &S) -> OpenActionResult<()> {
	let targets = settings.targets().to_vec();
	state::track(&instance.instance_id, serde_json::to_value(settings)?, targets.clone());
	render(instance).await;
	let id = instance.instance_id.clone();
	tokio::spawn(async move {
		state::sync_states(&targets, state::STATE_MAX_AGE).await;
		render_id(id).await;
	});
	Ok(())
}

pub fn disappeared(instance: &Instance) {
	state::untrack(&instance.instance_id);
}

/// Apply a command optimistically, then send it to every target, optionally
/// debounced (for dials). Shows an alert if nothing could be sent or every
/// device failed, and re-reads the real state after a failure.
pub async fn dispatch(
	instance: &Instance,
	targets: &[DeviceRef],
	command: Command,
	debounce: Option<Duration>,
) -> OpenActionResult<()> {
	if !state::has_key() {
		log::warn!("Action used before a Govee API key was set");
		return instance.show_alert().await;
	}
	if targets.is_empty() {
		log::warn!("Action has no devices selected");
		return instance.show_alert().await;
	}

	state::apply_optimistic(targets, command);
	render_all().await;

	let id = instance.instance_id.clone();
	let targets = targets.to_vec();
	let generation = debounce.map(|_| state::debounce_start(&id));
	tokio::spawn(async move {
		if let (Some(delay), Some(generation)) = (debounce, generation) {
			tokio::time::sleep(delay).await;
			if !state::debounce_is_current(&id, generation) {
				return;
			}
		}
		let outcome = state::send(&targets, command).await;
		if outcome.all_failed() {
			if let Some(instance) = openaction::get_instance(id).await {
				let _ = instance.show_alert().await;
			}
			state::sync_states(&targets, Duration::ZERO).await;
		} else if !outcome.errors.is_empty() {
			log::warn!("{} of {} device(s) failed: {:?}", outcome.errors.len(), targets.len(), outcome.errors);
		}
		render_all().await;
	});
	Ok(())
}

/// Toggle power for a group, based on the first device's state.
pub async fn toggle_power(instance: &Instance, targets: &[DeviceRef]) -> OpenActionResult<()> {
	let on = state::primary_state(targets).power.unwrap_or(false);
	dispatch(instance, targets, Command::Power(!on), None).await
}

fn settings_of<S: DeserializeOwned + Default>(value: &serde_json::Value) -> S {
	serde_json::from_value(value.clone()).unwrap_or_default()
}

/// Redraw one instance from its tracked settings and the cached device state.
pub async fn render(instance: &Instance) {
	let Some(tracked) = state::tracked(&instance.instance_id) else { return };
	let s = &tracked.settings;
	let result = match instance.action_uuid.as_str() {
		PowerAction::UUID => power::render(instance, &settings_of(s)).await,
		BrightnessAction::UUID => brightness::render(instance, &settings_of(s)).await,
		ColorAction::UUID => color::render(instance, &settings_of(s)).await,
		TemperatureAction::UUID => temperature::render(instance, &settings_of(s)).await,
		_ => Ok(()),
	};
	if let Err(error) = result {
		log::error!("Failed to render {}: {error}", instance.action_uuid);
	}
}

pub async fn render_id(id: String) {
	if let Some(instance) = openaction::get_instance(id).await {
		render(&instance).await;
	}
}

pub async fn render_all() {
	for id in state::tracked_ids() {
		render_id(id).await;
	}
}

/// Value line for the `$B1` dial layout: "Off" while the light is off.
pub fn dial_value_text(power: Option<bool>, text: String) -> String {
	if power == Some(false) { "Off".to_owned() } else { text }
}

/// Shared step/set mode for brightness and temperature keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
	#[default]
	Set,
	Step,
}
