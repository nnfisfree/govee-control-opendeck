//! Client for the official Govee Platform API (`https://openapi.api.govee.com`).
//!
//! Used by the OpenDeck plugin and the `govee-cli` developer tool. It has no
//! OpenDeck dependency, so it can be tested and reused on its own.
//!
//! API docs: <https://developer.govee.com/reference/get-you-devices>

mod client;
pub mod color;
mod ratelimit;
mod types;

pub use client::{Command, DEFAULT_BASE_URL, GoveeClient, GoveeError};
pub use ratelimit::{Bucket, Limit, RateLimiter};
pub use types::*;
