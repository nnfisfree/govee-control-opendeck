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

/// Key image for a device that Govee reports as offline: a crossed-out bulb.
pub fn offline_key() -> String {
	format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><rect width="144" height="144" rx="24" fill="#1c1c1e"/><path d="{BULB_PATH}" fill="none" stroke="#636366" stroke-width="5"/><rect x="58" y="110" width="28" height="6" rx="3" fill="#48484a"/><rect x="61" y="119" width="22" height="5" rx="2.5" fill="#48484a"/><path d="M36 30l72 84" stroke="#ff453a" stroke-width="8" stroke-linecap="round"/></svg>"##
	)
}

/// Key image showing a strip of segment colours, left to right.
pub fn segment_key(colors: &[u32]) -> String {
	let count = colors.len().clamp(1, 32);
	let width = 112.0 / count as f64;
	let mut cells = String::new();
	for (i, rgb) in colors.iter().take(count).enumerate() {
		let x = 16.0 + i as f64 * width;
		cells.push_str(&format!(
			r##"<rect x="{x:.2}" y="52" width="{w:.2}" height="40" fill="{hex}"/>"##,
			w = width + 0.5,
			hex = color::to_hex(*rgb)
		));
	}
	format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><rect width="144" height="144" rx="24" fill="#1c1c1e"/><clipPath id="c"><rect x="16" y="52" width="112" height="40" rx="8"/></clipPath><g clip-path="url(#c)">{cells}</g><rect x="16" y="52" width="112" height="40" rx="8" fill="none" stroke="#e5e5ea" stroke-width="3"/></svg>"##
	)
}

/// Key image with a large reading and an optional smaller second line.
pub fn reading_key(main: &str, sub: Option<&str>, accent: &str) -> String {
	let (main_y, sub_text) = match sub {
		Some(sub) => (
			70,
			format!(
				r##"<text x="72" y="108" font-family="system-ui,sans-serif" font-size="24" fill="#aeaeb2" text-anchor="middle">{}</text>"##,
				escape(sub)
			),
		),
		None => (84, String::new()),
	};
	format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><rect width="144" height="144" rx="24" fill="#1c1c1e"/><text x="72" y="{main_y}" font-family="system-ui,sans-serif" font-size="34" font-weight="600" fill="{accent}" text-anchor="middle">{}</text>{sub_text}</svg>"##,
		escape(main)
	)
}

fn escape(text: &str) -> String {
	text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
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

	#[test]
	fn generated_keys() {
		assert!(offline_key().contains("#ff453a"));
		let strip = segment_key(&[0xff0000, 0x00ff00]);
		assert!(strip.contains("#ff0000") && strip.contains("#00ff00"));
		let reading = reading_key("21.5°", Some("45% <rh>"), "#ff9f0a");
		assert!(reading.contains("21.5°") && reading.contains("&lt;rh&gt;"));
	}
}
