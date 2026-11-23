//! Token buckets that keep requests under Govee's documented rate limits, so
//! fast dial spinning or large groups queue up locally instead of getting 429s.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// A classic token bucket: `capacity` tokens, refilled at `per_second`.
#[derive(Clone, Debug)]
pub struct Bucket {
	capacity: f64,
	per_second: f64,
	tokens: f64,
	last: Instant,
}

impl Bucket {
	pub fn new(capacity: f64, per_second: f64, now: Instant) -> Self {
		Self { capacity, per_second, tokens: capacity, last: now }
	}

	/// Take one token if available. Otherwise return how long until one is.
	pub fn try_take(&mut self, now: Instant) -> Result<(), Duration> {
		let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
		self.tokens = (self.tokens + elapsed * self.per_second).min(self.capacity);
		self.last = now;
		if self.tokens >= 1.0 {
			self.tokens -= 1.0;
			Ok(())
		} else {
			Err(Duration::from_secs_f64((1.0 - self.tokens) / self.per_second))
		}
	}
}

/// Which documented limit a request counts against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limit {
	/// `/user/devices`: 30 per minute per account.
	DeviceList,
	/// `/device/control`: 12/s (burst 80) per account.
	ControlAccount,
	/// `/device/control`: 2/s (burst 6) per device.
	ControlDevice,
	/// `/device/state`: 30 per minute per device.
	State,
	/// `/device/scenes` and `/device/diy-scenes`: kept well below the state limit.
	SceneList,
}

impl Limit {
	fn params(self) -> (f64, f64) {
		match self {
			Limit::DeviceList | Limit::State => (30.0, 0.5),
			Limit::ControlAccount => (80.0, 12.0),
			Limit::ControlDevice => (6.0, 2.0),
			Limit::SceneList => (10.0, 0.2),
		}
	}
}

#[derive(Default)]
pub struct RateLimiter {
	buckets: Mutex<HashMap<(Limit, String), Bucket>>,
}

impl RateLimiter {
	/// Wait until a request against `limit` for `key` (a device ID, or "" for
	/// account-wide limits) is allowed, then consume the token.
	pub async fn acquire(&self, limit: Limit, key: &str) {
		loop {
			let wait = {
				let mut buckets = self.buckets.lock().await;
				let now = Instant::now();
				let (capacity, rate) = limit.params();
				let bucket = buckets.entry((limit, key.to_owned())).or_insert_with(|| Bucket::new(capacity, rate, now));
				match bucket.try_take(now) {
					Ok(()) => return,
					Err(wait) => wait,
				}
			};
			tokio::time::sleep(wait).await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bucket_allows_burst_then_throttles() {
		let start = Instant::now();
		let mut bucket = Bucket::new(6.0, 2.0, start);
		for _ in 0..6 {
			assert!(bucket.try_take(start).is_ok());
		}
		let wait = bucket.try_take(start).unwrap_err();
		assert!((wait.as_secs_f64() - 0.5).abs() < 1e-6);
		assert!(bucket.try_take(start + Duration::from_millis(500)).is_ok());
		assert!(bucket.try_take(start + Duration::from_millis(500)).is_err());
	}

	#[test]
	fn bucket_never_exceeds_capacity() {
		let start = Instant::now();
		let mut bucket = Bucket::new(2.0, 1.0, start);
		let later = start + Duration::from_secs(60);
		assert!(bucket.try_take(later).is_ok());
		assert!(bucket.try_take(later).is_ok());
		assert!(bucket.try_take(later).is_err());
	}

	#[tokio::test(start_paused = true)]
	async fn limiter_spaces_out_per_device_requests() {
		let limiter = RateLimiter::default();
		let start = Instant::now();
		for _ in 0..8 {
			limiter.acquire(Limit::ControlDevice, "dev").await;
		}
		// 6 burst tokens, then two more at 2/s → about 1s total.
		let elapsed = start.elapsed().as_secs_f64();
		assert!((0.99..1.1).contains(&elapsed), "elapsed {elapsed}");
		// A different device has its own bucket.
		let before = Instant::now();
		limiter.acquire(Limit::ControlDevice, "other").await;
		assert_eq!(before.elapsed(), Duration::ZERO);
	}
}
