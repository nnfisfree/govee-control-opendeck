//! Every action, and the lifecycle, dispatch and long-press code they share.
//!
//! Every action targets a list of devices (`devices` in its settings), so any
//! key or dial can control a single device or a group.

mod appliance;
mod brightness;
mod color;
mod cycle;
mod music;
mod power;
mod preset;
mod scene;
mod segment;
mod sensor;
mod temperature;
mod toggle;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use govee_api::{Command, DeviceRef, color as rgb};
use openaction::{Action, Instance, OpenActionResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub use appliance::ApplianceAction;
pub use brightness::BrightnessAction;
pub use color::ColorAction;
pub use cycle::CycleAction;
pub use music::MusicAction;
pub use power::PowerAction;
pub use preset::PresetAction;
pub use scene::SceneAction;
pub use segment::SegmentAction;
pub use sensor::SensorAction;
pub use temperature::TemperatureAction;
pub use toggle::ToggleAction;

use crate::render as img;
use crate::state::{self, Jobs};

/// Plugin bundle ID. Must match `bundle/manifest.json` and the folder name
/// `<bundle-id>.sdPlugin`. Never change it: it is part of saved profiles.
macro_rules! action_uuid {
	($name:literal) => {
		concat!("io.github.nnfisfree.govee-control", ".", $name)
	};
}
pub(crate) use action_uuid;

/// Delay between the last dial tick and the API call.
pub const DIAL_DEBOUNCE: Duration = Duration::from_millis(250);
/// Longer delay for dials that switch scenes, which take a moment to start.
pub const SCENE_DEBOUNCE: Duration = Duration::from_millis(600);
/// How long a key must be held for its long-press action.
pub const HOLD_TIME: Duration = Duration::from_millis(500);

/// Settings that name the devices an instance controls.
pub trait Targets {
	fn targets(&self) -> &[DeviceRef];
}

pub async fn register_all() {
	openaction::register_action(PowerAction).await;
	openaction::register_action(BrightnessAction).await;
	openaction::register_action(ColorAction).await;
	openaction::register_action(TemperatureAction).await;
	openaction::register_action(SceneAction).await;
	openaction::register_action(PresetAction).await;
	openaction::register_action(CycleAction).await;
	openaction::register_action(MusicAction).await;
	openaction::register_action(SegmentAction).await;
	openaction::register_action(ToggleAction).await;
	openaction::register_action(SensorAction).await;
	openaction::register_action(ApplianceAction).await;
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
	PRESSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&instance.instance_id);
}

/// Send one command to every target. See [`dispatch_jobs`].
pub async fn dispatch(
	instance: &Instance,
	targets: &[DeviceRef],
	command: Command,
	debounce: Option<Duration>,
) -> OpenActionResult<()> {
	dispatch_jobs(instance, state::jobs_for(targets, &[command]), debounce).await
}

/// Apply commands optimistically, then send them to every device, optionally
/// debounced (for dials). Shows an alert if nothing could be sent or every
/// device failed, and re-reads the real state after a failure.
pub async fn dispatch_jobs(instance: &Instance, jobs: Jobs, debounce: Option<Duration>) -> OpenActionResult<()> {
	if !state::has_key() {
		log::warn!("Action used before a Govee API key was set");
		return instance.show_alert().await;
	}
	let jobs: Jobs = jobs.into_iter().filter(|(_, commands)| !commands.is_empty()).collect();
	if jobs.is_empty() {
		log::warn!("Action has no devices selected, or nothing to send to them");
		return instance.show_alert().await;
	}

	state::apply_optimistic(&jobs);
	render_all().await;

	let id = instance.instance_id.clone();
	let generation = debounce.map(|_| state::debounce_start(&id));
	tokio::spawn(async move {
		if let (Some(delay), Some(generation)) = (debounce, generation) {
			tokio::time::sleep(delay).await;
			if !state::debounce_is_current(&id, generation) {
				return;
			}
		}
		let targets: Vec<DeviceRef> = jobs.iter().map(|(t, _)| t.clone()).collect();
		let count = jobs.len();
		let outcome = state::send(jobs).await;
		if outcome.all_failed() {
			if let Some(instance) = openaction::get_instance(id).await {
				let _ = instance.show_alert().await;
			}
			state::sync_states(&targets, Duration::ZERO).await;
		} else if !outcome.errors.is_empty() {
			log::warn!("{} of {} device(s) failed: {:?}", outcome.errors.len(), count, outcome.errors);
		}
		render_all().await;
	});
	Ok(())
}

/// Work out the jobs in the background (for example by loading scene lists),
/// then dispatch them. Keeps the event loop free while Govee answers.
pub fn dispatch_later<F>(instance: &Instance, debounce: Option<Duration>, jobs: F)
where
	F: std::future::Future<Output = Jobs> + Send + 'static,
{
	let id = instance.instance_id.clone();
	tokio::spawn(async move {
		let jobs = jobs.await;
		if let Some(instance) = openaction::get_instance(id).await
			&& let Err(error) = dispatch_jobs(&instance, jobs, debounce).await
		{
			log::error!("Dispatch failed: {error}");
		}
	});
}

/// Toggle power for a group, based on the first device's state.
pub async fn toggle_power(instance: &Instance, targets: &[DeviceRef]) -> OpenActionResult<()> {
	let on = state::primary_state(targets).power.unwrap_or(false);
	dispatch(instance, targets, Command::Power(!on), None).await
}

// ------------------------------------------------------------- long press ---

/// What a key does when it is held down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoldAction {
	#[default]
	None,
	Toggle,
	On,
	Off,
	Brightness,
	Color,
	Kelvin,
}

/// Long-press settings, shared by every key action (flattened into its settings).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hold {
	pub hold_action: HoldAction,
	pub hold_brightness: u8,
	pub hold_color: String,
	pub hold_kelvin: u32,
}

impl Default for Hold {
	fn default() -> Self {
		Self { hold_action: HoldAction::None, hold_brightness: 100, hold_color: "#ffffff".into(), hold_kelvin: 4000 }
	}
}

impl Hold {
	fn commands(&self, targets: &[DeviceRef]) -> Vec<Command> {
		match self.hold_action {
			HoldAction::None => Vec::new(),
			HoldAction::Toggle => vec![Command::Power(!state::primary_state(targets).power.unwrap_or(false))],
			HoldAction::On => vec![Command::Power(true)],
			HoldAction::Off => vec![Command::Power(false)],
			HoldAction::Brightness => {
				vec![Command::Power(true), Command::Brightness(self.hold_brightness.clamp(1, 100))]
			}
			HoldAction::Color => {
				vec![Command::Power(true), Command::Rgb(rgb::parse_hex(&self.hold_color).unwrap_or(0xffffff))]
			}
			HoldAction::Kelvin => {
				let range = state::kelvin_range(targets);
				vec![Command::Power(true), Command::Kelvin(self.hold_kelvin.clamp(range.0, range.1))]
			}
		}
	}
}

struct Press {
	generation: u64,
	fired: bool,
}

static PRESSES: LazyLock<Mutex<HashMap<String, Press>>> = LazyLock::new(Default::default);
static PRESS_GENERATION: Mutex<u64> = Mutex::new(0);

/// Call from `key_down`. Returns true if the action should run its normal
/// press now. With a long-press action set, the normal press waits for
/// `key_up` instead, and holding the key for [`HOLD_TIME`] runs the long-press
/// action.
pub fn key_down(instance: &Instance, targets: &[DeviceRef], hold: &Hold) -> bool {
	if hold.hold_action == HoldAction::None {
		return true;
	}
	let generation = {
		let mut g = PRESS_GENERATION.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
		*g += 1;
		*g
	};
	let id = instance.instance_id.clone();
	PRESSES
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner)
		.insert(id.clone(), Press { generation, fired: false });
	let targets = targets.to_vec();
	let hold = hold.clone();
	tokio::spawn(async move {
		tokio::time::sleep(HOLD_TIME).await;
		{
			let mut presses = PRESSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
			match presses.get_mut(&id) {
				Some(press) if press.generation == generation && !press.fired => press.fired = true,
				_ => return,
			}
		}
		if let Some(instance) = openaction::get_instance(id).await {
			let jobs = state::jobs_for(&targets, &hold.commands(&targets));
			if let Err(error) = dispatch_jobs(&instance, jobs, None).await {
				log::error!("Long press failed: {error}");
			}
		}
	});
	false
}

/// Call from `key_up`. Returns true if this was a short press that the action
/// should handle now (only when a long-press action is set).
pub fn key_up(instance: &Instance, hold: &Hold) -> bool {
	if hold.hold_action == HoldAction::None {
		return false;
	}
	let press = PRESSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&instance.instance_id);
	matches!(press, Some(Press { fired: false, .. }))
}

// -------------------------------------------------------------- rendering ---

fn settings_of<S: DeserializeOwned + Default>(value: &serde_json::Value) -> S {
	serde_json::from_value(value.clone()).unwrap_or_default()
}

/// Redraw one instance from its tracked settings and the cached device state.
/// An instance whose first device is offline is drawn as offline instead.
pub async fn render(instance: &Instance) {
	let Some(tracked) = state::tracked(&instance.instance_id) else { return };
	let offline = state::primary_state(&tracked.devices).online == Some(false);
	let was_offline = state::set_offline_shown(&instance.instance_id, offline);
	if offline {
		if let Err(error) = render_offline(instance).await {
			log::error!("Failed to render offline state: {error}");
		}
		return;
	}
	if was_offline && !is_encoder(instance) {
		// Back online: drop the offline image so the action's own image returns.
		let _ = instance.set_image(None::<String>, None).await;
		let _ = instance.set_title(None::<String>, None).await;
	}

	let s = &tracked.settings;
	let result = match instance.action_uuid.as_str() {
		PowerAction::UUID => power::render(instance, &settings_of(s)).await,
		BrightnessAction::UUID => brightness::render(instance, &settings_of(s)).await,
		ColorAction::UUID => color::render(instance, &settings_of(s)).await,
		TemperatureAction::UUID => temperature::render(instance, &settings_of(s)).await,
		SceneAction::UUID => scene::render(instance, &settings_of(s)).await,
		PresetAction::UUID => preset::render(instance, &settings_of(s)).await,
		CycleAction::UUID => cycle::render(instance, &settings_of(s)).await,
		MusicAction::UUID => music::render(instance, &settings_of(s)).await,
		SegmentAction::UUID => segment::render(instance, &settings_of(s)).await,
		ToggleAction::UUID => toggle::render(instance, &settings_of(s)).await,
		SensorAction::UUID => sensor::render(instance, &settings_of(s)).await,
		ApplianceAction::UUID => appliance::render(instance, &settings_of(s)).await,
		_ => Ok(()),
	};
	if let Err(error) = result {
		log::error!("Failed to render {}: {error}", instance.action_uuid);
	}
}

async fn render_offline(instance: &Instance) -> OpenActionResult<()> {
	if is_encoder(instance) {
		instance.set_feedback(&json!({ "value": "Offline", "indicator": { "value": 0 } })).await
	} else {
		instance.set_image(Some(img::data_url(&img::offline_key())), None).await?;
		instance.set_title(Some("Offline"), None).await
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

/// Break a name into short lines so it fits on a key (about 9 characters wide).
pub fn wrap_title(text: &str) -> String {
	const WIDTH: usize = 9;
	let mut lines: Vec<String> = Vec::new();
	for word in text.split_whitespace() {
		match lines.last_mut() {
			Some(line) if line.chars().count() + 1 + word.chars().count() <= WIDTH => {
				line.push(' ');
				line.push_str(word);
			}
			_ => lines.push(word.to_owned()),
		}
	}
	lines.truncate(3);
	lines.join("\n")
}

/// Move `current` by `ticks` positions in a list of `len` items, wrapping around.
pub fn wrap_index(current: usize, ticks: i64, len: usize) -> usize {
	if len == 0 {
		return 0;
	}
	(current as i64 + ticks).rem_euclid(len as i64) as usize
}

/// Shared step/set mode for brightness and temperature keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
	#[default]
	Set,
	Step,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn titles_wrap_to_key_width() {
		assert_eq!(wrap_title("Sunrise"), "Sunrise");
		assert_eq!(wrap_title("Northern lights glow"), "Northern\nlights\nglow");
		assert_eq!(wrap_title("A B C"), "A B C");
		assert_eq!(wrap_title("one two three four five six seven"), "one two\nthree\nfour five");
	}

	#[test]
	fn index_wraps_both_ways() {
		assert_eq!(wrap_index(0, 1, 3), 1);
		assert_eq!(wrap_index(2, 1, 3), 0);
		assert_eq!(wrap_index(0, -1, 3), 2);
		assert_eq!(wrap_index(1, -7, 3), 0);
		assert_eq!(wrap_index(5, 1, 0), 0);
	}

	#[test]
	fn hold_settings_flatten_with_defaults() {
		#[derive(Deserialize, Default)]
		#[serde(default)]
		struct S {
			#[serde(flatten)]
			hold: Hold,
		}
		let s: S = serde_json::from_value(json!({ "holdAction": "color", "holdColor": "#00ff00" })).unwrap();
		assert_eq!(s.hold.hold_action, HoldAction::Color);
		assert_eq!(s.hold.hold_color, "#00ff00");
		assert_eq!(s.hold.hold_brightness, 100);
		let s: S = serde_json::from_value(json!({})).unwrap();
		assert_eq!(s.hold.hold_action, HoldAction::None);
	}
}
