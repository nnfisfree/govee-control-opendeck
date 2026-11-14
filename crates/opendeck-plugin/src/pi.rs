//! Messages between the Property Inspector and the plugin.
//!
//! The PI never stores or reads the API key itself. It sends the key to the
//! plugin, which validates it against Govee, saves it in global settings and
//! only ever sends back a masked hint.
//!
//! PI → plugin (`sendToPlugin` payload):
//! - `{"type":"init"}`: ask for the current status and device list
//! - `{"type":"setApiKey","apiKey":"…"}`: validate and save a key
//! - `{"type":"clearApiKey"}`: forget the key
//! - `{"type":"refresh"}`: re-download the device list
//!
//! Plugin → PI (`sendToPropertyInspector` payload):
//! `{"type":"status","hasKey":bool,"keyHint":"••••abcd"|null,"devices":[…],"error":null|"…"}`

use openaction::{Instance, OpenActionResult};
use serde::Serialize;
use serde_json::Value;

use crate::state::{self, GlobalSettings};
use govee_api::Features;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PiDevice {
	sku: String,
	device: String,
	name: String,
	features: Features,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
	r#type: &'static str,
	has_key: bool,
	key_hint: Option<String>,
	devices: Vec<PiDevice>,
	error: Option<String>,
}

async fn send_status(instance: &Instance, error: Option<String>) -> OpenActionResult<()> {
	let devices = state::devices()
		.into_iter()
		.map(|d| PiDevice { name: d.display_name().to_owned(), features: d.features(), sku: d.sku, device: d.device })
		.collect();
	let status = Status {
		r#type: "status",
		has_key: state::has_key(),
		key_hint: state::key_hint(),
		devices,
		error: error.or_else(state::devices_error),
	};
	instance.send_to_property_inspector(status).await
}

/// Handle a `sendToPlugin` message. Network work runs in a background task so
/// the plugin's event loop is never blocked.
pub async fn handle(instance: &Instance, payload: &Value) -> OpenActionResult<()> {
	let kind = payload.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
	let api_key = payload.get("apiKey").and_then(Value::as_str).map(str::to_owned);
	let id = instance.instance_id.clone();

	tokio::spawn(async move {
		let error = match kind.as_str() {
			"init" => {
				state::devices_or_fetch().await;
				None
			}
			"refresh" => state::refresh_devices().await.err().map(|e| e.to_string()),
			"setApiKey" => set_api_key(api_key.unwrap_or_default()).await,
			"clearApiKey" => {
				let _ = state::set_api_key("");
				if let Err(error) = openaction::set_global_settings(GlobalSettings::default()).await {
					log::error!("Failed to clear global settings: {error}");
				}
				None
			}
			other => {
				log::warn!("Unknown PI message type '{other}'");
				return;
			}
		};
		if let Some(instance) = openaction::get_instance(id).await {
			if let Err(error) = send_status(&instance, error).await {
				log::error!("Failed to update property inspector: {error}");
			}
			crate::actions::render_all().await;
		}
	});
	Ok(())
}

/// Validate a key by listing devices with it and keep it only if that works.
async fn set_api_key(key: String) -> Option<String> {
	let key = key.trim().to_owned();
	if key.is_empty() {
		return Some("Please paste your Govee API key.".into());
	}
	let candidate = match govee_api::GoveeClient::new(&key) {
		Ok(client) => client,
		Err(error) => return Some(error.to_string()),
	};
	let devices = match candidate.devices().await {
		Ok(devices) => devices,
		Err(error) => return Some(error.to_string()),
	};
	if let Err(error) = state::set_api_key(&key) {
		return Some(error.to_string());
	}
	state::set_devices(devices);
	if let Err(error) = openaction::set_global_settings(GlobalSettings { api_key: key }).await {
		log::error!("Failed to save global settings: {error}");
	}
	log::info!("Govee API key updated");
	None
}
