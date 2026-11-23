//! Plugin-wide state shared by every action: the Govee client (built from the
//! API key in global settings), the device list, cached device states and the
//! settings of every visible action instance.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;

use govee_api::{Command, Device, DeviceRef, DeviceState, GoveeClient, GoveeError, NamedValue, SceneKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinSet;
use tokio::time::Instant;

/// How long a fetched device state is considered fresh.
pub const STATE_MAX_AGE: Duration = Duration::from_secs(50);
/// How often visible instances are re-synced with the cloud state.
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Stored with `setGlobalSettings`. OpenDeck keeps this in plain text in its
/// config directory; the README tells users so.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalSettings {
	pub api_key: String,
}

/// What we remember about each visible action instance.
#[derive(Clone, Debug)]
pub struct Tracked {
	pub settings: Value,
	pub devices: Vec<DeviceRef>,
}

#[derive(Default)]
struct Inner {
	client: Option<Arc<GoveeClient>>,
	devices: Vec<Device>,
	devices_error: Option<String>,
	states: HashMap<String, (DeviceState, Option<Instant>)>,
	instances: HashMap<String, Tracked>,
	debounce: HashMap<String, u64>,
	/// Scene lists by device ID and kind.
	scenes: HashMap<(String, SceneKind), Vec<NamedValue>>,
	/// Per-instance position in a list (cycle keys, scene and colour dials).
	cursors: HashMap<String, usize>,
	/// Per-instance text chosen on a dial (e.g. the scene it is on).
	labels: HashMap<String, String>,
	/// Instances currently drawn as "offline".
	offline_shown: HashSet<String>,
}

/// Commands to send to each device. Most actions send the same commands to
/// every device, but scenes and music modes are looked up per model.
pub type Jobs = Vec<(DeviceRef, Vec<Command>)>;

/// The same commands for every target.
pub fn jobs_for(targets: &[DeviceRef], commands: &[Command]) -> Jobs {
	targets.iter().map(|t| (t.clone(), commands.to_vec())).collect()
}

static STATE: LazyLock<RwLock<Inner>> = LazyLock::new(|| RwLock::new(Inner::default()));
/// Serialises device-list refreshes so several PIs opening at once trigger one request.
static REFRESH_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
static GENERATION: Mutex<u64> = Mutex::new(0);

fn read() -> std::sync::RwLockReadGuard<'static, Inner> {
	STATE.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write() -> std::sync::RwLockWriteGuard<'static, Inner> {
	STATE.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ---------------------------------------------------------------- API key ---

pub fn client() -> Option<Arc<GoveeClient>> {
	read().client.clone()
}

pub fn has_key() -> bool {
	read().client.is_some()
}

/// Last four characters of the key, for display in the PI.
pub fn key_hint() -> Option<String> {
	read().client.as_ref().map(|c| {
		let key = c.api_key();
		let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
		format!("••••{tail}")
	})
}

/// Install a new API key. Returns true if it differs from the current one.
/// An empty key clears the client and the device list.
pub fn set_api_key(key: &str) -> Result<bool, GoveeError> {
	let key = key.trim();
	let mut s = write();
	if s.client.as_ref().map(|c| c.api_key()) == Some(key) || (key.is_empty() && s.client.is_none()) {
		return Ok(false);
	}
	if key.is_empty() {
		s.client = None;
	} else {
		s.client = Some(Arc::new(GoveeClient::new(key)?));
	}
	s.devices.clear();
	s.devices_error = None;
	s.states.clear();
	s.scenes.clear();
	Ok(true)
}

// ------------------------------------------------------------ device list ---

pub fn devices() -> Vec<Device> {
	read().devices.clone()
}

pub fn set_devices(devices: Vec<Device>) {
	let mut s = write();
	s.devices = devices;
	s.devices_error = None;
}

pub fn device(target: &DeviceRef) -> Option<Device> {
	read().devices.iter().find(|d| d.device == target.device && d.sku == target.sku).cloned()
}

pub fn devices_error() -> Option<String> {
	read().devices_error.clone()
}

/// Fetch the device list from Govee and cache it.
pub async fn refresh_devices() -> Result<Vec<Device>, GoveeError> {
	let _guard = REFRESH_LOCK.lock().await;
	let Some(client) = client() else {
		return Ok(Vec::new());
	};
	let result = client.devices().await;
	let mut s = write();
	// Ignore the result if the key changed while we were waiting.
	if s.client.as_ref().map(|c| c.api_key()) != Some(client.api_key()) {
		return Ok(s.devices.clone());
	}
	match &result {
		Ok(devices) => {
			log::info!("Loaded {} Govee device(s)", devices.len());
			s.devices = devices.clone();
			s.devices_error = None;
			// Scene lists and snapshots may have changed in the Govee app.
			s.scenes.clear();
		}
		Err(error) => {
			log::warn!("Failed to load Govee devices: {error}");
			s.devices_error = Some(error.to_string());
		}
	}
	result
}

/// Return cached devices, fetching them first if the cache is empty.
pub async fn devices_or_fetch() -> Vec<Device> {
	let cached = devices();
	if !cached.is_empty() || !has_key() {
		return cached;
	}
	refresh_devices().await.unwrap_or_default()
}

pub fn kelvin_range(targets: &[DeviceRef]) -> (u32, u32) {
	let s = read();
	let mut range: Option<(u32, u32)> = None;
	for d in s.devices.iter().filter(|d| targets.iter().any(|t| t.device == d.device)) {
		let f = d.features();
		if !f.kelvin {
			continue;
		}
		range = Some(match range {
			None => (f.kelvin_min, f.kelvin_max),
			// Use the range every device in the group supports.
			Some((lo, hi)) => (lo.max(f.kelvin_min), hi.min(f.kelvin_max)),
		});
	}
	match range {
		Some((lo, hi)) if lo < hi => (lo, hi),
		_ => govee_api::DEFAULT_KELVIN_RANGE,
	}
}

// ----------------------------------------------------------------- scenes ---

/// Scenes of one kind for one device, from the cache or from Govee.
/// Snapshots come from the device list instead of a separate request.
pub async fn scene_list(target: &DeviceRef, kind: SceneKind) -> Result<Vec<NamedValue>, GoveeError> {
	let key = (target.device.clone(), kind);
	if let Some(cached) = read().scenes.get(&key) {
		return Ok(cached.clone());
	}
	let list = match kind {
		SceneKind::Snapshot => {
			devices_or_fetch().await;
			device(target).map(|d| d.snapshots()).unwrap_or_default()
		}
		_ => {
			let Some(client) = client() else { return Ok(Vec::new()) };
			client.scenes(target, kind).await?
		}
	};
	write().scenes.insert(key, list.clone());
	Ok(list)
}

/// The value of the scene called `name` on `target`, matched case-insensitively.
pub async fn find_scene(target: &DeviceRef, kind: SceneKind, name: &str) -> Option<serde_json::Value> {
	match scene_list(target, kind).await {
		Ok(list) => list.into_iter().find(|s| s.name.trim().eq_ignore_ascii_case(name.trim())).map(|s| s.value),
		Err(error) => {
			log::warn!("Could not load scenes for {}: {error}", target.device);
			None
		}
	}
}

/// Scene names available on every target, in the first target's order.
pub async fn common_scene_names(targets: &[DeviceRef], kind: SceneKind) -> Result<Vec<String>, GoveeError> {
	let mut names: Option<Vec<String>> = None;
	for target in targets {
		let list: Vec<String> = scene_list(target, kind).await?.into_iter().map(|s| s.name).collect();
		names = Some(match names {
			None => list,
			Some(names) => names.into_iter().filter(|n| list.iter().any(|l| l.eq_ignore_ascii_case(n))).collect(),
		});
	}
	Ok(names.unwrap_or_default())
}

// ---------------------------------------------------------------- cursors ---

pub fn cursor(instance_id: &str) -> Option<usize> {
	read().cursors.get(instance_id).copied()
}

pub fn set_cursor(instance_id: &str, position: usize) {
	write().cursors.insert(instance_id.to_owned(), position);
}

pub fn label(instance_id: &str) -> Option<String> {
	read().labels.get(instance_id).cloned()
}

pub fn set_label(instance_id: &str, label: &str) {
	write().labels.insert(instance_id.to_owned(), label.to_owned());
}

/// Remember whether an instance is drawn as offline. Returns the previous value.
pub fn set_offline_shown(instance_id: &str, offline: bool) -> bool {
	let mut s = write();
	if offline { !s.offline_shown.insert(instance_id.to_owned()) } else { s.offline_shown.remove(instance_id) }
}

// ---------------------------------------------------------- device states ---

pub fn state(device: &DeviceRef) -> DeviceState {
	read().states.get(&device.device).map(|(s, _)| s.clone()).unwrap_or_default()
}

/// State of the first device in a group: groups are shown and toggled as one.
pub fn primary_state(targets: &[DeviceRef]) -> DeviceState {
	targets.first().map(state).unwrap_or_default()
}

/// Apply commands to the cached state without talking to Govee
/// (used for instant feedback while a dial is turning).
pub fn apply_optimistic(jobs: &Jobs) {
	let mut s = write();
	for (target, commands) in jobs {
		let entry = s.states.entry(target.device.clone()).or_default();
		for command in commands {
			command.apply_to(&mut entry.0);
		}
	}
}

/// Fetch states for devices whose cached state is older than `max_age`.
pub async fn sync_states(targets: &[DeviceRef], max_age: Duration) {
	let Some(client) = client() else { return };
	let now = Instant::now();
	let stale: Vec<DeviceRef> = {
		let s = read();
		let mut seen = HashSet::new();
		targets
			.iter()
			.filter(|t| seen.insert(t.device.clone()))
			.filter(|t| match s.states.get(&t.device) {
				Some((_, Some(at))) => now.duration_since(*at) > max_age,
				_ => true,
			})
			.cloned()
			.collect()
	};
	let mut set = JoinSet::new();
	for target in stale {
		let client = client.clone();
		set.spawn(async move { (client.state(&target).await, target) });
	}
	while let Some(Ok((result, target))) = set.join_next().await {
		match result {
			Ok(state) => {
				write().states.insert(target.device, (state, Some(Instant::now())));
			}
			Err(error) => log::warn!("State query for {} failed: {error}", target.device),
		}
	}
}

/// Result of sending one command to a group of devices.
#[derive(Debug, Default)]
pub struct Outcome {
	pub ok: usize,
	pub errors: Vec<String>,
}

impl Outcome {
	pub fn all_failed(&self) -> bool {
		self.ok == 0 && !self.errors.is_empty()
	}
}

/// Send each device its commands, in order. Devices are handled concurrently;
/// the rate limiter inside the client keeps large groups within Govee's limits.
/// A device counts as OK only if all its commands succeeded.
pub async fn send(jobs: Jobs) -> Outcome {
	let Some(client) = client() else {
		return Outcome { ok: 0, errors: vec!["no Govee API key configured".into()] };
	};
	let mut set = JoinSet::new();
	for (target, commands) in jobs {
		let client = client.clone();
		set.spawn(async move {
			for command in &commands {
				if let Err(error) = client.control(&target, command).await {
					log::warn!("{command:?} → {} failed: {error}", target.device);
					return Err(error.to_string());
				}
				let mut s = write();
				let entry = s.states.entry(target.device.clone()).or_default();
				command.apply_to(&mut entry.0);
			}
			Ok(())
		});
	}
	let mut outcome = Outcome::default();
	while let Some(joined) = set.join_next().await {
		match joined {
			Ok(Ok(())) => outcome.ok += 1,
			Ok(Err(error)) => outcome.errors.push(error),
			Err(error) => outcome.errors.push(error.to_string()),
		}
	}
	outcome
}

// -------------------------------------------------------------- instances ---

pub fn track(instance_id: &str, settings: Value, devices: Vec<DeviceRef>) {
	write().instances.insert(instance_id.to_owned(), Tracked { settings, devices });
}

pub fn untrack(instance_id: &str) {
	let mut s = write();
	s.instances.remove(instance_id);
	s.debounce.remove(instance_id);
	s.cursors.remove(instance_id);
	s.labels.remove(instance_id);
	s.offline_shown.remove(instance_id);
}

pub fn tracked(instance_id: &str) -> Option<Tracked> {
	read().instances.get(instance_id).cloned()
}

pub fn tracked_ids() -> Vec<String> {
	read().instances.keys().cloned().collect()
}

/// Every device used by any visible instance.
pub fn all_targets() -> Vec<DeviceRef> {
	let s = read();
	let mut seen = HashSet::new();
	s.instances.values().flat_map(|t| t.devices.iter()).filter(|d| seen.insert(d.device.clone())).cloned().collect()
}

// --------------------------------------------------------------- debounce ---

/// Start a new debounce generation for `key` and return it.
pub fn debounce_start(key: &str) -> u64 {
	let next = {
		let mut g = GENERATION.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
		*g += 1;
		*g
	};
	write().debounce.insert(key.to_owned(), next);
	next
}

/// True if `generation` is still the latest one for `key`.
pub fn debounce_is_current(key: &str, generation: u64) -> bool {
	read().debounce.get(key) == Some(&generation)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn debounce_generations() {
		let a = debounce_start("inst-a");
		assert!(debounce_is_current("inst-a", a));
		let b = debounce_start("inst-a");
		assert!(!debounce_is_current("inst-a", a));
		assert!(debounce_is_current("inst-a", b));
		untrack("inst-a");
		assert!(!debounce_is_current("inst-a", b));
	}

	#[test]
	fn optimistic_updates_and_targets() {
		let lamp = DeviceRef { sku: "H6601".into(), device: "test-optimistic".into() };
		apply_optimistic(&jobs_for(std::slice::from_ref(&lamp), &[Command::Brightness(30)]));
		assert_eq!(state(&lamp).brightness, Some(30));
		apply_optimistic(&jobs_for(std::slice::from_ref(&lamp), &[Command::Power(false)]));
		assert_eq!(primary_state(std::slice::from_ref(&lamp)).power, Some(false));

		track("inst-t1", Value::Null, vec![lamp.clone()]);
		track("inst-t2", Value::Null, vec![lamp.clone()]);
		assert_eq!(all_targets().iter().filter(|d| d.device == lamp.device).count(), 1);
		untrack("inst-t1");
		untrack("inst-t2");
	}

	#[test]
	fn cursors_and_offline_flags() {
		set_cursor("inst-c", 3);
		assert_eq!(cursor("inst-c"), Some(3));
		assert!(!set_offline_shown("inst-c", true));
		assert!(set_offline_shown("inst-c", true));
		untrack("inst-c");
		assert_eq!(cursor("inst-c"), None);
		assert!(!set_offline_shown("inst-c", false));
	}
}
