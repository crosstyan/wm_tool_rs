use log::{info, warn};
use anyhow::Ok;
use anyhow::{anyhow, bail, Result};
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use serialport::{available_ports, SerialPort};
use clap::Parser;
use clap::builder::TypedValueParser;

pub mod flash;

use flash::*;

const DOWNLOAD_BAUD_RATES: &'static [&'static str] = &["115200", "460800", "921600", "1000000", "2000000"];

#[derive(Parser, Debug)]
#[command(name = "wm_tool", version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    /// Serial port name, in the form of `/dev/ttyUSB0` (Linux) or `COM1` (Windows) or `/dev/cu.usbserial-0001` (macOS)
    port: String,
    #[arg(short, long, default_value = "115200")]
    /// Baud rate for the normal communication (protocol handshake etc.)
    wire_baud_rate: u32,
    // https://github.com/clap-rs/clap/discussions/3855
    #[arg(short, long, default_value = "2000000", value_parser = clap::builder::PossibleValuesParser::new(DOWNLOAD_BAUD_RATES).map(| s | s.parse::< u32 > ().unwrap()))]
    /// Baud rate for the image download
    download_baud_rate: u32,
    #[arg(short, long)]
    /// Path to the image file, should be in the format of `*.fls`
    image_path: String,
}

fn main() -> Result<()> {
    use std::result::Result::Ok;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "wm_tool=debug");
    }
    env_logger::init();
    let args = Args::parse();
    let port_ = serialport::new(args.port.clone(), args.wire_baud_rate).open_native();
    let mut port = match port_ {
        Ok(port) => port,
        Err(e) => {
            let ports = available_ports().unwrap_or(vec![]);
            warn!("Available ports: {:#?}", ports);
            bail!("Failed to open serial port `{}` since {}", &args.port, e)
        }
    };
    rts_reset(&mut port)?;
    escape_2_uart(&mut port, Duration::from_millis(500))?;
    chk_magics(&mut port)?;
    let mac = query_mac(&mut port)?;
    info!(
        "MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let image_path = args.image_path;
    info!("Image path: {}", image_path);
    let mut image_file = File::open(image_path)?;
    let total_bytes = image_file.metadata()?.len();
    let expected_iterations = (total_bytes as f32 / XMODEM_DATA_SIZE as f32).ceil() as u64;

    info!("Image size: {} bytes", total_bytes);
    info!("Expected iterations: {}", expected_iterations);
    erase_image(&mut port)?;
    chk_magics(&mut port)?;
    set_download_speed(&mut port, args.download_baud_rate)?;
    let bar = indicatif::ProgressBar::new(expected_iterations);
    write_image(&mut port, &mut image_file, |_| {
        bar.inc(1);
    })?;
    bar.finish();
    println!();
    info!("Image written successfully");

    std::thread::sleep(Duration::from_millis(100));
    rts_reset(&mut port)?;
    info!("Redirect the UART output from {}", &args.port);
    port.set_baud_rate(args.wire_baud_rate)?;
    log_uart(port);
    Ok(())
}
