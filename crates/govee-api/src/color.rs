//! Colour conversions used by the plugin: packed RGB integers (Govee's
//! `colorRgb` format), hex strings, hue and colour temperature.

/// Pack an RGB triple into Govee's `colorRgb` integer: `(r << 16) + (g << 8) + b`.
pub fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
	((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Split a packed `colorRgb` integer into its RGB components.
pub fn unpack_rgb(rgb: u32) -> (u8, u8, u8) {
	(((rgb >> 16) & 0xff) as u8, ((rgb >> 8) & 0xff) as u8, (rgb & 0xff) as u8)
}

/// Parse `#rrggbb` or `rrggbb` (case-insensitive) into a packed RGB integer.
pub fn parse_hex(hex: &str) -> Option<u32> {
	let hex = hex.trim().trim_start_matches('#');
	if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
		return None;
	}
	u32::from_str_radix(hex, 16).ok()
}

/// Format a packed RGB integer as `#rrggbb`.
pub fn to_hex(rgb: u32) -> String {
	format!("#{:06x}", rgb & 0xff_ffff)
}

/// Convert a hue in degrees (any value, wrapped to 0..360) at full saturation
/// and value to a packed RGB integer.
pub fn hue_to_rgb(hue: f64) -> u32 {
	hsv_to_rgb(hue, 1.0, 1.0)
}

/// Convert HSV (hue in degrees, saturation and value in 0..=1) to packed RGB.
pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> u32 {
	let h = hue.rem_euclid(360.0) / 60.0;
	let s = saturation.clamp(0.0, 1.0);
	let v = value.clamp(0.0, 1.0);
	let c = v * s;
	let x = c * (1.0 - (h % 2.0 - 1.0).abs());
	let m = v - c;
	let (r, g, b) = match h as u32 {
		0 => (c, x, 0.0),
		1 => (x, c, 0.0),
		2 => (0.0, c, x),
		3 => (0.0, x, c),
		4 => (x, 0.0, c),
		_ => (c, 0.0, x),
	};
	let to_u8 = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
	pack_rgb(to_u8(r), to_u8(g), to_u8(b))
}

/// Hue in degrees (0..360) of a packed RGB colour. Greys return 0.
pub fn rgb_to_hue(rgb: u32) -> f64 {
	let (r, g, b) = unpack_rgb(rgb);
	let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
	let max = r.max(g).max(b);
	let min = r.min(g).min(b);
	let delta = max - min;
	if delta == 0.0 {
		return 0.0;
	}
	let hue = if max == r {
		60.0 * ((g - b) / delta).rem_euclid(6.0)
	} else if max == g {
		60.0 * ((b - r) / delta + 2.0)
	} else {
		60.0 * ((r - g) / delta + 4.0)
	};
	hue.rem_euclid(360.0)
}

/// Approximate display colour of a black-body light at `kelvin`, used only to
/// tint key images. Based on Tanner Helland's well-known curve fit.
pub fn kelvin_to_rgb(kelvin: u32) -> u32 {
	let t = (kelvin.clamp(1000, 40000) as f64) / 100.0;
	let r = if t <= 66.0 { 255.0 } else { 329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2) };
	let g = if t <= 66.0 {
		99.470_802_586_1 * t.ln() - 161.119_568_166_1
	} else {
		288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
	};
	let b = if t >= 66.0 {
		255.0
	} else if t <= 19.0 {
		0.0
	} else {
		138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
	};
	let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
	pack_rgb(c(r), c(g), c(b))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn pack_unpack_roundtrip() {
		assert_eq!(pack_rgb(255, 255, 255), 16_777_215);
		assert_eq!(pack_rgb(0x12, 0x34, 0x56), 0x123456);
		assert_eq!(unpack_rgb(0x123456), (0x12, 0x34, 0x56));
	}

	#[test]
	fn hex_parsing() {
		assert_eq!(parse_hex("#FF8800"), Some(0xff8800));
		assert_eq!(parse_hex("00ff00"), Some(0x00ff00));
		assert_eq!(parse_hex("#fff"), None);
		assert_eq!(parse_hex("#gg0000"), None);
		assert_eq!(to_hex(0xff8800), "#ff8800");
		assert_eq!(to_hex(0x0000ff), "#0000ff");
	}

	#[test]
	fn hue_conversions() {
		assert_eq!(hue_to_rgb(0.0), 0xff0000);
		assert_eq!(hue_to_rgb(120.0), 0x00ff00);
		assert_eq!(hue_to_rgb(240.0), 0x0000ff);
		assert_eq!(hue_to_rgb(360.0), 0xff0000);
		assert_eq!(hue_to_rgb(-120.0), 0x0000ff);
		for hue in [0.0, 30.0, 90.0, 180.0, 270.0, 330.0] {
			let back = rgb_to_hue(hue_to_rgb(hue));
			assert!((back - hue).abs() < 1.0, "hue {hue} came back as {back}");
		}
		assert_eq!(rgb_to_hue(0x808080), 0.0);
	}

	#[test]
	fn kelvin_tints() {
		let (r, _, b) = unpack_rgb(kelvin_to_rgb(2000));
		assert!(r > b, "warm light should be red-heavy");
		let (r, _, b) = unpack_rgb(kelvin_to_rgb(9000));
		assert!(b >= r, "cool light should be blue-heavy");
	}
}
