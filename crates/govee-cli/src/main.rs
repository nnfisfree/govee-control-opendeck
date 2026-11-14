//! Developer tool for exercising the Govee API with a real key.
//! Not part of the shipped plugin.
//!
//! The key is read from the `GOVEE_API_KEY` environment variable, so it doesn't
//! end up in your shell history:
//!
//! ```sh
//! read -rs GOVEE_API_KEY && export GOVEE_API_KEY
//! cargo run -p govee-cli -- devices
//! cargo run -p govee-cli -- state "Desk lamp"
//! cargo run -p govee-cli -- power "Desk lamp" on
//! cargo run -p govee-cli -- brightness "Desk lamp" 40
//! cargo run -p govee-cli -- color "Desk lamp" "#ff8800"
//! cargo run -p govee-cli -- kelvin "Desk lamp" 4000
//! ```

use std::process::ExitCode;

use govee_api::color;
use govee_api::{Command, Device, GoveeClient};

const USAGE: &str = "usage: govee-cli <devices | state DEVICE | power DEVICE on|off | brightness DEVICE 1-100 | color DEVICE #rrggbb | kelvin DEVICE K>
DEVICE is a device name or device ID. The API key is read from GOVEE_API_KEY.";

#[tokio::main]
async fn main() -> ExitCode {
	match run(std::env::args().skip(1).collect()).await {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("error: {error}");
			ExitCode::FAILURE
		}
	}
}

async fn run(args: Vec<String>) -> Result<(), String> {
	let key = std::env::var("GOVEE_API_KEY").map_err(|_| "set GOVEE_API_KEY first".to_owned())?;
	let client = GoveeClient::new(key).map_err(|e| e.to_string())?;
	let args: Vec<&str> = args.iter().map(String::as_str).collect();

	match args.as_slice() {
		["devices"] => {
			let devices = client.devices().await.map_err(|e| e.to_string())?;
			if devices.is_empty() {
				println!("No devices found on this account.");
			}
			for d in devices {
				let f = d.features();
				println!(
					"{:<24} sku={:<7} id={}  power={} brightness={} rgb={} kelvin={}",
					d.display_name(),
					d.sku,
					d.device,
					f.power,
					f.brightness,
					f.rgb,
					if f.kelvin { format!("{}-{}K", f.kelvin_min, f.kelvin_max) } else { "no".into() }
				);
			}
		}
		["state", device] => {
			let d = find(&client, device).await?;
			let s = client.state(&d.reference()).await.map_err(|e| e.to_string())?;
			println!(
				"{}: online={:?} power={:?} brightness={:?} rgb={} kelvin={:?}",
				d.display_name(),
				s.online,
				s.power,
				s.brightness,
				s.rgb.map(color::to_hex).unwrap_or_else(|| "None".into()),
				s.kelvin
			);
		}
		["power", device, value] => {
			let on = match *value {
				"on" | "1" => true,
				"off" | "0" => false,
				_ => return Err(USAGE.into()),
			};
			control(&client, device, Command::Power(on)).await?;
		}
		["brightness", device, value] => {
			let v: u8 = value.parse().map_err(|_| "brightness must be 1-100".to_owned())?;
			control(&client, device, Command::Brightness(v)).await?;
		}
		["color", device, value] => {
			let rgb = color::parse_hex(value).ok_or("color must look like #rrggbb")?;
			control(&client, device, Command::Rgb(rgb)).await?;
		}
		["kelvin", device, value] => {
			let k: u32 = value.parse().map_err(|_| "kelvin must be a number".to_owned())?;
			control(&client, device, Command::Kelvin(k)).await?;
		}
		_ => return Err(USAGE.into()),
	}
	Ok(())
}

async fn find(client: &GoveeClient, needle: &str) -> Result<Device, String> {
	let devices = client.devices().await.map_err(|e| e.to_string())?;
	devices
		.into_iter()
		.find(|d| d.device.eq_ignore_ascii_case(needle) || d.display_name().eq_ignore_ascii_case(needle))
		.ok_or_else(|| format!("no device named or with ID '{needle}' (run `govee-cli devices`)"))
}

async fn control(client: &GoveeClient, device: &str, command: Command) -> Result<(), String> {
	let d = find(client, device).await?;
	client.control(&d.reference(), command).await.map_err(|e| e.to_string())?;
	println!("OK: {:?} → {}", command, d.display_name());
	Ok(())
}
