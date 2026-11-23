//! Cycle: each press moves to the next item in a list of colours, brightness
//! levels, white temperatures or scenes. A dial moves through the list both ways.

use govee_api::color;
use govee_api::{Command, DeviceRef, DeviceState, SceneKind};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::scene::scene_jobs;
use super::{DIAL_DEBOUNCE, Hold, SCENE_DEBOUNCE, Targets, action_uuid};
use crate::{render as img, state};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleKind {
	#[default]
	Color,
	Brightness,
	Kelvin,
	Scene,
	Diy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CycleSettings {
	pub devices: Vec<DeviceRef>,
	pub kind: CycleKind,
	/// Colours, numbers or scene names. Scenes go one per line; the others may
	/// also be separated by commas or spaces.
	pub items: String,
	pub show_value: bool,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for CycleSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			kind: CycleKind::Color,
			items: "#ff0000, #30d158, #0a84ff".into(),
			show_value: true,
			hold: Hold::default(),
		}
	}
}

impl Targets for CycleSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
	Rgb(u32),
	Brightness(u8),
	Kelvin(u32),
	Scene(SceneKind, String),
}

impl Item {
	fn matches(&self, state: &DeviceState) -> bool {
		match self {
			Item::Rgb(rgb) => state.kelvin.is_none() && state.rgb == Some(*rgb),
			Item::Brightness(b) => state.brightness == Some(*b),
			Item::Kelvin(k) => state.kelvin == Some(*k),
			Item::Scene(..) => false,
		}
	}

	fn label(&self) -> String {
		match self {
			Item::Rgb(rgb) => color::to_hex(*rgb).to_uppercase(),
			Item::Brightness(b) => format!("{b}%"),
			Item::Kelvin(k) => format!("{k}K"),
			Item::Scene(_, name) => name.clone(),
		}
	}
}

/// Parse the item list. Invalid entries are skipped.
pub fn parse_items(kind: CycleKind, text: &str) -> Vec<Item> {
	let words = || text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|w| !w.is_empty());
	match kind {
		CycleKind::Color => words().filter_map(color::parse_hex).map(Item::Rgb).collect(),
		CycleKind::Brightness => words()
			.filter_map(|w| w.trim_end_matches('%').parse::<u8>().ok())
			.map(|b| Item::Brightness(b.clamp(1, 100)))
			.collect(),
		CycleKind::Kelvin => {
			words().filter_map(|w| w.trim_end_matches(['K', 'k']).parse::<u32>().ok()).map(Item::Kelvin).collect()
		}
		CycleKind::Scene | CycleKind::Diy => {
			let scene_kind = if kind == CycleKind::Diy { SceneKind::Diy } else { SceneKind::Scene };
			text.lines()
				.map(str::trim)
				.filter(|l| !l.is_empty())
				.map(|l| Item::Scene(scene_kind, l.to_owned()))
				.collect()
		}
	}
}

/// Index of the item the key is on: the last one it applied, or the one that
/// matches the light's current state.
fn current_index(instance: &Instance, items: &[Item], state: &DeviceState) -> Option<usize> {
	state::cursor(&instance.instance_id)
		.filter(|&i| i < items.len())
		.or_else(|| items.iter().position(|item| item.matches(state)))
}

pub struct CycleAction;

impl CycleAction {
	async fn step(
		&self,
		instance: &Instance,
		settings: &CycleSettings,
		ticks: i64,
		dial: bool,
	) -> OpenActionResult<()> {
		let items = parse_items(settings.kind, &settings.items);
		if items.is_empty() {
			log::warn!("Cycle key has no valid items");
			return instance.show_alert().await;
		}
		let state = state::primary_state(&settings.devices);
		let next = match current_index(instance, &items, &state) {
			Some(i) => super::wrap_index(i, ticks, items.len()),
			None if ticks >= 0 => 0,
			None => items.len() - 1,
		};
		state::set_cursor(&instance.instance_id, next);
		let debounce = |delay| if dial { Some(delay) } else { None };
		let command = match &items[next] {
			Item::Rgb(rgb) => Command::Rgb(*rgb),
			Item::Brightness(b) => Command::Brightness(*b),
			Item::Kelvin(k) => {
				let range = state::kelvin_range(&settings.devices);
				Command::Kelvin((*k).clamp(range.0, range.1))
			}
			Item::Scene(kind, name) => {
				// Scene values are looked up per light, so they are sent in the background.
				super::render(instance).await;
				let jobs = scene_jobs(settings.devices.clone(), *kind, name.clone());
				super::dispatch_later(instance, debounce(SCENE_DEBOUNCE), jobs);
				return Ok(());
			}
		};
		super::dispatch(instance, &settings.devices, command, debounce(DIAL_DEBOUNCE)).await
	}
}

#[async_trait]
impl Action for CycleAction {
	const UUID: &'static str = action_uuid!("cycle");
	type Settings = CycleSettings;

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
			self.step(instance, settings, 1, false).await?;
		}
		Ok(())
	}

	async fn key_up(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		if super::key_up(instance, &settings.hold) {
			self.step(instance, settings, 1, false).await?;
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
		self.step(instance, settings, ticks as i64, true).await
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

pub async fn render(instance: &Instance, settings: &CycleSettings) -> OpenActionResult<()> {
	let items = parse_items(settings.kind, &settings.items);
	let state = state::primary_state(&settings.devices);
	let current = current_index(instance, &items, &state).map(|i| &items[i]);
	if super::is_encoder(instance) {
		let text = match current {
			Some(item) => {
				format!("{} ({}/{})", item.label(), items.iter().position(|i| i == item).unwrap_or(0) + 1, items.len())
			}
			None => "Turn to pick".into(),
		};
		let mut feedback = json!({ "title": "Cycle", "value": super::dial_value_text(state.power, text) });
		match current {
			Some(Item::Rgb(rgb)) => feedback["icon"] = json!(img::dial_icon(*rgb)),
			Some(Item::Kelvin(k)) => feedback["icon"] = json!(img::dial_icon(color::kelvin_to_rgb(*k))),
			_ => {}
		}
		return instance.set_feedback(&feedback).await;
	}

	match current {
		Some(Item::Rgb(rgb)) => instance.set_image(Some(img::data_url(&img::bulb_key(*rgb, false))), None).await?,
		Some(Item::Kelvin(k)) => {
			instance.set_image(Some(img::data_url(&img::bulb_key(color::kelvin_to_rgb(*k), false))), None).await?
		}
		_ => instance.set_image(None::<String>, None).await?,
	}
	let title = match current {
		Some(item @ (Item::Brightness(_) | Item::Kelvin(_) | Item::Scene(..))) if settings.show_value => {
			Some(super::wrap_title(&item.label()))
		}
		_ => None,
	};
	instance.set_title(title, None).await
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_each_kind() {
		assert_eq!(
			parse_items(CycleKind::Color, "#ff0000, 00ff00 nope"),
			vec![Item::Rgb(0xff0000), Item::Rgb(0x00ff00)]
		);
		assert_eq!(
			parse_items(CycleKind::Brightness, "10%, 50 100 0"),
			vec![Item::Brightness(10), Item::Brightness(50), Item::Brightness(100), Item::Brightness(1)]
		);
		assert_eq!(parse_items(CycleKind::Kelvin, "2700K 6500"), vec![Item::Kelvin(2700), Item::Kelvin(6500)]);
		assert_eq!(
			parse_items(CycleKind::Scene, "Sunrise\n  Northern lights \n\n"),
			vec![
				Item::Scene(SceneKind::Scene, "Sunrise".into()),
				Item::Scene(SceneKind::Scene, "Northern lights".into())
			]
		);
		assert_eq!(parse_items(CycleKind::Diy, "Party"), vec![Item::Scene(SceneKind::Diy, "Party".into())]);
	}

	#[test]
	fn items_match_state() {
		let state = DeviceState { rgb: Some(0xff0000), brightness: Some(50), ..Default::default() };
		assert!(Item::Rgb(0xff0000).matches(&state));
		assert!(Item::Brightness(50).matches(&state));
		assert!(!Item::Kelvin(4000).matches(&state));
		let white = DeviceState { rgb: Some(0xff0000), kelvin: Some(4000), ..Default::default() };
		assert!(!Item::Rgb(0xff0000).matches(&white), "a light in white mode is not showing its last colour");
	}
}
