//! Govee Control plugin for OpenDeck.
//!
//! OpenDeck starts this binary with `-port -pluginUUID -registerEvent -info`.
//! The `openaction` crate connects to OpenDeck over WebSocket and routes events
//! to the actions in `actions/`. Logs go to stderr, which OpenDeck writes to
//! `<OpenDeck log dir>/plugins/<plugin-uuid>.log`.

mod actions;
mod pi;
mod render;
mod state;

use openaction::global_events::{
	DidReceiveGlobalSettingsEvent, GlobalEventHandler, SystemDidWakeUpEvent, set_global_event_handler,
};
use openaction::{OpenActionResult, async_trait};
use simplelog::{ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};
use state::GlobalSettings;

struct Globals;

#[async_trait]
impl GlobalEventHandler for Globals {
	async fn plugin_ready(&self) -> OpenActionResult<()> {
		log::info!("Govee plugin {} connected to OpenDeck", env!("CARGO_PKG_VERSION"));
		openaction::get_global_settings().await?;
		tokio::spawn(poll_states());
		Ok(())
	}

	async fn did_receive_global_settings(&self, event: DidReceiveGlobalSettingsEvent) -> OpenActionResult<()> {
		let settings: GlobalSettings = serde_json::from_value(event.payload.settings).unwrap_or_default();
		match state::set_api_key(&settings.api_key) {
			Ok(true) => {
				log::info!("Govee API key loaded from global settings");
				tokio::spawn(async {
					let _ = state::refresh_devices().await;
					resync(std::time::Duration::ZERO).await;
				});
			}
			Ok(false) => {}
			Err(error) => log::error!("Could not create Govee client: {error}"),
		}
		Ok(())
	}

	async fn system_did_wake_up(&self, _event: SystemDidWakeUpEvent) -> OpenActionResult<()> {
		tokio::spawn(resync(std::time::Duration::ZERO));
		Ok(())
	}
}

/// Refresh the state of every device shown on the deck, then redraw.
async fn resync(max_age: std::time::Duration) {
	if !state::has_key() {
		return;
	}
	state::sync_states(&state::all_targets(), max_age).await;
	actions::render_all().await;
}

/// Keep keys in sync with changes made outside OpenDeck (the Govee app, a
/// physical switch). The interval keeps us far below the 30/min/device limit.
async fn poll_states() {
	let mut interval = tokio::time::interval(state::POLL_INTERVAL);
	interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
	interval.tick().await;
	loop {
		interval.tick().await;
		resync(state::STATE_MAX_AGE).await;
	}
}

static GLOBALS: Globals = Globals;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
	let config = ConfigBuilder::new().add_filter_allow_str("govee").build();
	let level = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info };
	let _ = TermLogger::init(level, config, TerminalMode::Stderr, ColorChoice::Never);

	set_global_event_handler(&GLOBALS);
	actions::register_all().await;
	openaction::run(std::env::args().collect()).await
}
