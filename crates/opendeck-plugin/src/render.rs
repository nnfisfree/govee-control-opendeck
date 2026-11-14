//! Generated key and dial images (SVG).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use govee_api::color;

/// Wrap SVG markup as a base64 data URL for `setImage`.
pub fn data_url(svg: &str) -> String {
	format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg))
}

const BULB_PATH: &str = "M72 30c-17.7 0-32 14.3-32 32 0 11.6 6.2 21.8 15.5 27.4V100c0 3.3 2.7 6 6 6h21c3.3 0 6-2.7 6-6V89.4C97.8 83.8 104 73.6 104 62c0-17.7-14.3-32-32-32z";

/// 144×144 key image: a bulb filled with `rgb` on a dark rounded square.
/// `dimmed` draws the bulb as an outline (light off).
pub fn bulb_key(rgb: u32, dimmed: bool) -> String {
	let hex = color::to_hex(rgb);
	let fill = if dimmed { "none".to_owned() } else { hex.clone() };
	format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><rect width="144" height="144" rx="24" fill="#1c1c1e"/><path d="{BULB_PATH}" fill="{fill}" stroke="{hex}" stroke-width="5"/><rect x="58" y="110" width="28" height="6" rx="3" fill="#8e8e93"/><rect x="61" y="119" width="22" height="5" rx="2.5" fill="#8e8e93"/></svg>"##
	)
}

/// Small (48×48) swatch for the encoder layout's `icon` slot. OpenDeck's strip
/// renderer accepts raw SVG markup here.
pub fn dial_icon(rgb: u32) -> String {
	let hex = color::to_hex(rgb);
	format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 144 144"><path d="{BULB_PATH}" fill="{hex}"/><rect x="58" y="110" width="28" height="6" rx="3" fill="#c7c7cc"/><rect x="61" y="119" width="22" height="5" rx="2.5" fill="#c7c7cc"/></svg>"##
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn svg_contains_color_and_encodes() {
		let svg = bulb_key(0xff8800, false);
		assert!(svg.contains("#ff8800"));
		assert!(data_url(&svg).starts_with("data:image/svg+xml;base64,PHN2Zy"));
		assert!(bulb_key(0xff8800, true).contains(r#"fill="none""#));
		assert!(dial_icon(0x00ff00).starts_with("<svg"));
	}
}
