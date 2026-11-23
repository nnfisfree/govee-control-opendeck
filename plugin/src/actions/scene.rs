//! Scene: play a built-in Govee scene, a DIY scene or a saved snapshot.
//!
//! Scene values differ between light models, so the settings store the scene
//! *name* and each light in a group looks up its own value by name.

use govee_api::{Command, DeviceRef, SceneKind};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Hold, SCENE_DEBOUNCE, Targets, action_uuid};
use crate::state::{self, Jobs};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneSettings {
	pub devices: Vec<DeviceRef>,
	pub source: SceneKind,
	/// Scene name, as listed by Govee.
	pub scene: String,
	/// Show the scene name as the key title.
	pub show_name: bool,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for SceneSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			source: SceneKind::Scene,
			scene: String::new(),
			show_name: true,
			hold: Hold::default(),
		}
	}
}

impl Targets for SceneSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// Look up the scene called `name` on every target. Lights that don't have it
/// are skipped (and logged).
pub async fn scene_jobs(targets: Vec<DeviceRef>, kind: SceneKind, name: String) -> Jobs {
	let mut jobs = Vec::new();
	for target in targets {
		match state::find_scene(&target, kind, &name).await {
			Some(value) => jobs.push((target, vec![Command::Scene { kind, value }])),
			None => log::warn!("Scene '{name}' is not available on {}", target.device),
		}
	}
	jobs
}

pub fn source_label(kind: SceneKind) -> &'static str {
	match kind {
		SceneKind::Scene => "Scene",
		SceneKind::Diy => "DIY scene",
		SceneKind::Snapshot => "Snapshot",
	}
}

/// The scene a dial is on, or the configured one.
fn shown_scene(instance: &Instance, settings: &SceneSettings) -> String {
	state::label(&instance.instance_id).unwrap_or_else(|| settings.scene.clone())
}

pub struct SceneAction;

impl SceneAction {
	async fn press(&self, instance: &Instance, settings: &SceneSettings) -> OpenActionResult<()> {
		if settings.scene.trim().is_empty() {
			log::warn!("Scene key has no scene selected");
			return instance.show_alert().await;
		}
		super::dispatch_later(
			instance,
			None,
			scene_jobs(settings.devices.clone(), settings.source, settings.scene.clone()),
		);
		Ok(())
	}
}

#[async_trait]
impl Action for SceneAction {
	const UUID: &'static str = action_uuid!("scene");
	type Settings = SceneSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		// Warm the scene cache so the first press is quick.
		let (targets, kind) = (settings.devices.clone(), settings.source);
		tokio::spawn(async move {
			for target in targets {
				let _ = state::scene_list(&target, kind).await;
			}
		});
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

	/// Turning the dial browses the scenes every selected light has and plays
	/// the one it stops on.
	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		if settings.devices.is_empty() || !state::has_key() {
			return instance.show_alert().await;
		}
		let id = instance.instance_id.clone();
		let settings = settings.clone();
		tokio::spawn(async move {
			let names = match state::common_scene_names(&settings.devices, settings.source).await {
				Ok(names) if !names.is_empty() => names,
				Ok(_) => {
					log::warn!("The selected lights have no {} in common", source_label(settings.source));
					return;
				}
				Err(error) => {
					log::warn!("Could not load scenes: {error}");
					return;
				}
			};
			let current = state::cursor(&id)
				.filter(|&i| i < names.len())
				.or_else(|| names.iter().position(|n| n.eq_ignore_ascii_case(&settings.scene)));
			let next = match current {
				Some(i) => super::wrap_index(i, ticks as i64, names.len()),
				None if ticks > 0 => 0,
				None => names.len() - 1,
			};
			state::set_cursor(&id, next);
			state::set_label(&id, &names[next]);
			super::render_id(id.clone()).await;
			let jobs = scene_jobs(settings.devices.clone(), settings.source, names[next].clone()).await;
			if let Some(instance) = openaction::get_instance(id).await {
				let _ = super::dispatch_jobs(&instance, jobs, Some(SCENE_DEBOUNCE)).await;
			}
		});
		Ok(())
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

pub async fn render(instance: &Instance, settings: &SceneSettings) -> OpenActionResult<()> {
	let scene = shown_scene(instance, settings);
	if super::is_encoder(instance) {
		let power = state::primary_state(&settings.devices).power;
		let text = if scene.is_empty() { "Turn to pick".to_owned() } else { scene };
		instance
			.set_feedback(&json!({
				"title": source_label(settings.source),
				"value": super::dial_value_text(power, text),
			}))
			.await
	} else if settings.show_name && !scene.is_empty() {
		instance.set_title(Some(super::wrap_title(&scene)), None).await
	} else {
		instance.set_title(None::<String>, None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn settings_parse_source() {
		let s: SceneSettings = serde_json::from_value(json!({ "source": "diy", "scene": "Party" })).unwrap();
		assert_eq!(s.source, SceneKind::Diy);
		assert_eq!(s.scene, "Party");
		assert!(s.show_name);
		let s: SceneSettings = serde_json::from_value(json!({})).unwrap();
		assert_eq!(s.source, SceneKind::Scene);
	}
}
