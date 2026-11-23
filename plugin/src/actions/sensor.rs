//! Sensor: show the temperature and humidity from a Govee thermometer or
//! hygrometer. The reading refreshes every minute; press to refresh now.

use std::time::Duration;

use govee_api::DeviceRef;
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Targets, action_uuid};
use crate::{render as img, state};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SensorShow {
	#[default]
	Both,
	Temperature,
	Humidity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureUnit {
	#[default]
	C,
	F,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SensorSettings {
	pub devices: Vec<DeviceRef>,
	pub show: SensorShow,
	pub unit: TemperatureUnit,
}

impl Targets for SensorSettings {
	fn targets(&self) -> &[DeviceRef] {
		&self.devices
	}
}

/// Convert a reading to the display unit.
pub fn convert(value: f64, source_celsius: bool, unit: TemperatureUnit) -> f64 {
	match (source_celsius, unit) {
		(true, TemperatureUnit::F) => value * 9.0 / 5.0 + 32.0,
		(false, TemperatureUnit::C) => (value - 32.0) * 5.0 / 9.0,
		_ => value,
	}
}

/// The formatted temperature and humidity of the first selected sensor.
fn readings(settings: &SensorSettings) -> (Option<String>, Option<String>) {
	let Some(target) = settings.devices.first() else { return (None, None) };
	let state = state::state(target);
	let celsius = state::device(target).is_some_and(|d| d.sensor_celsius());
	let unit = match settings.unit {
		TemperatureUnit::C => "C",
		TemperatureUnit::F => "F",
	};
	let temperature = state.sensor_temperature.map(|t| format!("{:.1}°{unit}", convert(t, celsius, settings.unit)));
	let humidity = state.sensor_humidity.map(|h| format!("{h:.0}%"));
	(temperature, humidity)
}

pub struct SensorAction;

impl SensorAction {
	async fn refresh(&self, instance: &Instance, settings: &SensorSettings) -> OpenActionResult<()> {
		if settings.devices.is_empty() || !state::has_key() {
			return instance.show_alert().await;
		}
		let id = instance.instance_id.clone();
		let targets = settings.devices.clone();
		tokio::spawn(async move {
			state::sync_states(&targets, Duration::ZERO).await;
			super::render_id(id).await;
		});
		Ok(())
	}
}

#[async_trait]
impl Action for SensorAction {
	const UUID: &'static str = action_uuid!("sensor");
	type Settings = SensorSettings;

	async fn will_appear(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		// The temperature unit comes from the device list.
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
		self.refresh(instance, settings).await
	}

	async fn dial_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		self.refresh(instance, settings).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		self.refresh(instance, settings).await
	}
}

pub async fn render(instance: &Instance, settings: &SensorSettings) -> OpenActionResult<()> {
	let (temperature, humidity) = readings(settings);
	let dash = || "–".to_owned();
	if super::is_encoder(instance) {
		let text = match settings.show {
			SensorShow::Both => format!("{} · {}", temperature.unwrap_or_else(dash), humidity.unwrap_or_else(dash)),
			SensorShow::Temperature => temperature.unwrap_or_else(dash),
			SensorShow::Humidity => humidity.unwrap_or_else(dash),
		};
		return instance.set_feedback(&json!({ "title": "Sensor", "value": text })).await;
	}
	let svg = match settings.show {
		SensorShow::Both => {
			img::reading_key(&temperature.unwrap_or_else(dash), humidity.as_deref().or(Some("–")), "#ff9f0a")
		}
		SensorShow::Temperature => img::reading_key(&temperature.unwrap_or_else(dash), None, "#ff9f0a"),
		SensorShow::Humidity => img::reading_key(&humidity.unwrap_or_else(dash), None, "#64d2ff"),
	};
	instance.set_image(Some(img::data_url(&svg)), None).await
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn unit_conversion() {
		assert!((convert(212.0, false, TemperatureUnit::C) - 100.0).abs() < 1e-9);
		assert!((convert(100.0, true, TemperatureUnit::F) - 212.0).abs() < 1e-9);
		assert_eq!(convert(21.5, true, TemperatureUnit::C), 21.5);
		assert_eq!(convert(70.0, false, TemperatureUnit::F), 70.0);
	}
}
