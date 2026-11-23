//! Segment colour: colour or dim individual segments of a light strip or bar,
//! with one colour or a gradient between two colours.

use govee_api::color;
use govee_api::{Command, DeviceRef};
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DIAL_DEBOUNCE, Hold, Targets, action_uuid};
use crate::render as img;
use crate::state::{self, Jobs};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentStyle {
	/// One colour on every chosen segment.
	#[default]
	Solid,
	/// Blend from `color` to `color2` across the chosen segments.
	Gradient,
	/// Set the brightness of the chosen segments.
	Brightness,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SegmentSettings {
	pub devices: Vec<DeviceRef>,
	/// Segments, numbered from 1: "all", or a list like "1-5, 8".
	pub segments: String,
	pub style: SegmentStyle,
	pub color: String,
	pub color2: String,
	pub brightness: u8,
	/// Hue degrees (colour styles) or percent (brightness style) per dial tick.
	pub dial_step: i32,
	#[serde(flatten)]
	pub hold: Hold,
}

impl Default for SegmentSettings {
	fn default() -> Self {
		Self {
			devices: Vec::new(),
			segments: "all".into(),
			style: SegmentStyle::Solid,
			color: "#ff0000".into(),
			color2: "#0000ff".into(),
			brightness: 100,
			dial_step: 10,
			hold: Hold::default(),
		}
	}
}

impl Targets for SegmentSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// Parse a 1-based segment list ("all", "1-5, 8") into sorted 0-based indices
/// below `count`.
pub fn parse_segments(text: &str, count: u32) -> Vec<u32> {
	let text = text.trim();
	if text.is_empty() || text.eq_ignore_ascii_case("all") {
		return (0..count).collect();
	}
	let mut out = Vec::new();
	for part in text.split([',', ';', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
		let (start, end) = match part.split_once('-') {
			Some((a, b)) => (a.trim().parse::<u32>().ok(), b.trim().parse::<u32>().ok()),
			None => {
				let n = part.parse::<u32>().ok();
				(n, n)
			}
		};
		if let (Some(a), Some(b)) = (start, end) {
			let (a, b) = (a.min(b).max(1), a.max(b));
			out.extend((a..=b).map(|n| n - 1).filter(|&i| i < count));
		}
	}
	out.sort_unstable();
	out.dedup();
	out
}

/// Linear blend between two packed RGB colours, `t` in 0..=1.
pub fn blend(a: u32, b: u32, t: f64) -> u32 {
	let (ar, ag, ab) = color::unpack_rgb(a);
	let (br, bg, bb) = color::unpack_rgb(b);
	let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t.clamp(0.0, 1.0)).round() as u8;
	color::pack_rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
}

fn shift_hue(rgb: u32, degrees: usize) -> u32 {
	if degrees.is_multiple_of(360) { rgb } else { color::hue_to_rgb(color::rgb_to_hue(rgb) + degrees as f64) }
}

/// Colour of each chosen segment, in order.
fn segment_colors(settings: &SegmentSettings, segments: &[u32], hue: usize) -> Vec<u32> {
	let a = shift_hue(color::parse_hex(&settings.color).unwrap_or(0xff0000), hue);
	let b = shift_hue(color::parse_hex(&settings.color2).unwrap_or(0x0000ff), hue);
	match settings.style {
		SegmentStyle::Gradient if segments.len() > 1 => {
			let last = (segments.len() - 1) as f64;
			(0..segments.len()).map(|i| blend(a, b, i as f64 / last)).collect()
		}
		_ => vec![a; segments.len()],
	}
}

/// Commands for one light: one per run of segments that share a colour.
pub fn segment_commands(settings: &SegmentSettings, count: u32, hue: usize, brightness: u8) -> Vec<Command> {
	let segments = parse_segments(&settings.segments, count);
	if segments.is_empty() {
		return Vec::new();
	}
	if settings.style == SegmentStyle::Brightness {
		return vec![Command::SegmentBrightness { segments, brightness: brightness.min(100) }];
	}
	let colors = segment_colors(settings, &segments, hue);
	let mut commands: Vec<Command> = Vec::new();
	for (segment, rgb) in segments.into_iter().zip(colors) {
		match commands.last_mut() {
			Some(Command::SegmentRgb { segments, rgb: last }) if *last == rgb => segments.push(segment),
			_ => commands.push(Command::SegmentRgb { segments: vec![segment], rgb }),
		}
	}
	commands
}

fn segment_count(target: &DeviceRef) -> u32 {
	state::device(target).map(|d| d.segment_count()).unwrap_or(0)
}

fn jobs(settings: &SegmentSettings, hue: usize, brightness: u8) -> Jobs {
	settings
		.devices
		.iter()
		.map(|t| (t.clone(), segment_commands(settings, segment_count(t), hue, brightness)))
		.collect()
}

/// The dial's position: a hue shift in degrees, or a brightness in percent.
fn dial_position(instance: &Instance, settings: &SegmentSettings) -> usize {
	state::cursor(&instance.instance_id).unwrap_or(match settings.style {
		SegmentStyle::Brightness => settings.brightness as usize,
		_ => 0,
	})
}

pub struct SegmentAction;

impl SegmentAction {
	async fn press(&self, instance: &Instance, settings: &SegmentSettings) -> OpenActionResult<()> {
		super::dispatch_jobs(instance, jobs(settings, 0, settings.brightness), None).await
	}
}

#[async_trait]
impl Action for SegmentAction {
	const UUID: &'static str = action_uuid!("segment");
	type Settings = SegmentSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
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

	/// Turning the dial shifts the hue of the segments, or changes their
	/// brightness in brightness style.
	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let step = ticks as i64 * settings.dial_step.max(1) as i64;
		let current = dial_position(instance, settings) as i64;
		let (hue, brightness, position) = match settings.style {
			SegmentStyle::Brightness => {
				let b = (current + step).clamp(1, 100);
				(0, b as u8, b as usize)
			}
			_ => {
				let h = (current + step).rem_euclid(360) as usize;
				(h, settings.brightness, h)
			}
		};
		state::set_cursor(&instance.instance_id, position);
		super::dispatch_jobs(instance, jobs(settings, hue, brightness), Some(DIAL_DEBOUNCE)).await
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

/// Preview colours for every segment of the first light (10 if unknown).
fn preview(settings: &SegmentSettings, hue: usize, brightness: u8) -> Vec<u32> {
	let count = settings.devices.first().map(segment_count).filter(|&c| c > 0).unwrap_or(10);
	let chosen = parse_segments(&settings.segments, count);
	let colors = match settings.style {
		SegmentStyle::Brightness => {
			let v = (brightness.clamp(1, 100) as f64 / 100.0 * 255.0).round() as u8;
			vec![color::pack_rgb(v, v, v); chosen.len()]
		}
		_ => segment_colors(settings, &chosen, hue),
	};
	(0..count).map(|i| chosen.iter().position(|&c| c == i).map(|p| colors[p]).unwrap_or(0x3a3a3c)).collect()
}

pub async fn render(instance: &Instance, settings: &SegmentSettings) -> OpenActionResult<()> {
	if super::is_encoder(instance) {
		let position = dial_position(instance, settings);
		let power = state::primary_state(&settings.devices).power;
		let (text, indicator, icon) = match settings.style {
			SegmentStyle::Brightness => (format!("{position}%"), position, img::dial_icon(0xffffff)),
			_ => {
				let rgb = shift_hue(color::parse_hex(&settings.color).unwrap_or(0xff0000), position);
				(color::to_hex(rgb).to_uppercase(), position * 100 / 360, img::dial_icon(rgb))
			}
		};
		instance
			.set_feedback(&json!({
				"title": "Segments",
				"value": super::dial_value_text(power, text),
				"indicator": { "value": indicator },
				"icon": icon,
			}))
			.await
	} else {
		let svg = img::segment_key(&preview(settings, 0, settings.brightness));
		instance.set_image(Some(img::data_url(&svg)), None).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn segment_lists() {
		assert_eq!(parse_segments("all", 4), vec![0, 1, 2, 3]);
		assert_eq!(parse_segments("", 2), vec![0, 1]);
		assert_eq!(parse_segments("1-3, 5", 10), vec![0, 1, 2, 4]);
		assert_eq!(parse_segments("3-1 2", 10), vec![0, 1, 2]);
		assert_eq!(parse_segments("8-20", 10), vec![7, 8, 9]);
		assert_eq!(parse_segments("0, x, 4", 10), vec![3]);
	}

	#[test]
	fn blends_colors() {
		assert_eq!(blend(0x000000, 0xffffff, 0.0), 0x000000);
		assert_eq!(blend(0x000000, 0xffffff, 1.0), 0xffffff);
		assert_eq!(blend(0xff0000, 0x0000ff, 0.5), 0x800080);
	}

	#[test]
	fn commands_group_equal_colors() {
		let solid = SegmentSettings { segments: "1-3".into(), ..Default::default() };
		assert_eq!(
			segment_commands(&solid, 10, 0, 100),
			vec![Command::SegmentRgb { segments: vec![0, 1, 2], rgb: 0xff0000 }]
		);

		let gradient = SegmentSettings {
			segments: "1-3".into(),
			style: SegmentStyle::Gradient,
			color: "#000000".into(),
			color2: "#ffffff".into(),
			..Default::default()
		};
		let commands = segment_commands(&gradient, 10, 0, 100);
		assert_eq!(commands.len(), 3);
		assert_eq!(commands[1], Command::SegmentRgb { segments: vec![1], rgb: 0x808080 });

		let dim = SegmentSettings { style: SegmentStyle::Brightness, ..Default::default() };
		assert_eq!(
			segment_commands(&dim, 3, 0, 40),
			vec![Command::SegmentBrightness { segments: vec![0, 1, 2], brightness: 40 }]
		);
		assert!(segment_commands(&solid, 0, 0, 100).is_empty());
	}
}
