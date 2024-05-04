use anyhow::Ok;
use anyhow::{anyhow, bail, Result};
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use serialport::SerialPort;

pub mod flash;

use flash::*;


fn main() -> Result<()> {
    let mut port = serialport::new("/dev/ttyUSB0", 115_200)
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
    let image_path = "/home/crosstyan/Code/wm806-cmake/build/demo.fls";
    // let image_path = "/home/crosstyan/Code/wm-sdk-w806/bin/W806/W806.fls";
    let mut image_file = File::open(image_path)?;
    erase_image(&mut port)?;
    chk_magics(&mut port)?;
    set_download_speed(&mut port, 2000000)?;
    write_image(&mut port, &mut image_file)?;
    std::thread::sleep(Duration::from_secs(1));
    rts_reset(&mut port)?;
    println!("Done");
    port.set_baud_rate(115200)?;
    std::thread::sleep(Duration::from_millis(500));
    cmd_reset(&mut port)?;
    log_uart(port);
    Ok(())
}
