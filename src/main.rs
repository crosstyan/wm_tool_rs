use anyhow::Ok;
use anyhow::{anyhow, bail, Result};
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use serialport::SerialPort;
use clap::Parser;

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
    #[arg(short, long, default_value = "2000000", value_parser = clap::builder::PossibleValuesParser::new(DOWNLOAD_BAUD_RATES))]
    /// Baud rate for the image download
    download_baud_rate: u32,
    #[arg(short, long)]
    /// Path to the image file, should be in the format of `*.fls`
    image_path: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut port = serialport::new(args.port, args.wire_baud_rate)
        .timeout(Duration::from_millis(30_000))
        .open_native()?;
    rts_reset(&mut port)?;
    escape_2_uart(&mut port, Duration::from_millis(500))?;
    chk_magics(&mut port)?;
    let mac = query_mac(&mut port)?;
    println!(
        "MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let image_path = args.image_path;
    let mut image_file = File::open(image_path)?;
    erase_image(&mut port)?;
    chk_magics(&mut port)?;
    set_download_speed(&mut port, args.download_baud_rate)?;
    write_image(&mut port, &mut image_file)?;
    std::thread::sleep(Duration::from_secs(1));
    rts_reset(&mut port)?;
    println!("Done");
    port.set_baud_rate(args.wire_baud_rate)?;
    std::thread::sleep(Duration::from_millis(500));
    cmd_reset(&mut port)?;
    log_uart(port);
    Ok(())
}
