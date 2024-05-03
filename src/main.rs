use anyhow::Ok;
use anyhow::{anyhow, bail, Result};
use crc16::{State, AUG_CCITT};
use serialport::{available_ports, ClearBuffer, SerialPort, TTYPort};
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::io::Write;
use std::time::Duration;

const WM_TOOL_PATH_MAX: usize = 256;
const WM_TOOL_ONCE_READ_LEN: usize = 1024;
const WM_TOOL_RUN_IMG_HEADER_LEN: usize = 0x100;
const WM_TOOL_SECBOOT_IMG_ADDR: usize = 0x2100;
const WM_TOOL_SECBOOT_HEADER_LEN: usize = 0x100;
const WM_TOOL_SECBOOT_HEADER_POS: usize = WM_TOOL_SECBOOT_IMG_ADDR - WM_TOOL_SECBOOT_HEADER_LEN;
const WM_TOOL_IMG_HEAD_MAGIC_NO: u32 = 0xA0FFFF9F;
const WM_TOOL_DEFAULT_BAUD_RATE: u32 = 115200;
const WM_TOOL_DOWNLOAD_TIMEOUT_SEC: u64 = 60 * 1;
const WM_TOOL_USE_1K_XMODEM: bool = true;
const WM_TOOL_IMAGE_VERSION_LEN: usize = 16;

const XMODEM_SOH: u8 = 0x01;
const XMODEM_STX: u8 = 0x02;
const XMODEM_EOT: u8 = 0x04;
const XMODEM_ACK: u8 = 0x06;
const XMODEM_NAK: u8 = 0x15;
const XMODEM_CAN: u8 = 0x18;
const XMODEM_CRC_CHR: u8 = b'C';
const XMODEM_CRC_SIZE: usize = 2;
const XMODEM_FRAME_ID_SIZE: usize = 2;
const XMODEM_DATA_SIZE_SOH: usize = 128;
const XMODEM_DATA_SIZE_STX: usize = 1024;
const XMODEM_MAGIC_SIZE: usize = 1;
const XMODEM_HEADER_SIZE: usize = XMODEM_MAGIC_SIZE + XMODEM_FRAME_ID_SIZE;
const XMODEM_TAIL_SIZE: usize = XMODEM_CRC_SIZE;
const XMODEM_FRAME_SIZE: usize = XMODEM_DATA_SIZE + XMODEM_HEADER_SIZE + XMODEM_TAIL_SIZE;

const XMODEM_DATA_SIZE: usize = if WM_TOOL_USE_1K_XMODEM {
    XMODEM_DATA_SIZE_STX
} else {
    XMODEM_DATA_SIZE_SOH
};

const XMODEM_HEAD: u8 = if WM_TOOL_USE_1K_XMODEM {
    XMODEM_STX
} else {
    XMODEM_SOH
};

fn pr() {
    let ps = available_ports().expect("No serial ports found!");
    for p in ps {
        println!("{}", p.port_name);
    }
}

fn log_uart(mut port: TTYPort) {
    use std::result::Result::Ok;
    port.write_request_to_send(false)
        .expect("Failed to set RTS");
    loop {
        let mut buf: Vec<u8> = vec![0; 100];
        match port.read(buf.as_mut_slice()) {
            Ok(t) => {
                for i in 0..t {
                    print!("{:02X} ", buf[i]);
                }
                println!();
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
            Err(e) => eprintln!("{:?}", e),
        }
    }
}

fn try_reset(port: &mut TTYPort) -> Result<()> {
    // DTR 0
    // RTS 1
    port.write_data_terminal_ready(false)?;
    port.write_request_to_send(true)?;
    const SLEEP_TIME: Duration = Duration::from_millis(100);
    std::thread::sleep(SLEEP_TIME);
    // DTR 1
    // RTS 0
    port.write_data_terminal_ready(true)?;
    port.write_request_to_send(false)?;
    std::thread::sleep(SLEEP_TIME);
    // DTR 0
    // RTS 0
    port.write_data_terminal_ready(false)?;
    port.write_request_to_send(false)?;
    Ok(())
}

fn chk_magic(port: &mut TTYPort) -> Result<(bool, char)> {
    let mut buf: [u8; 1] = [0];
    port.read_exact(&mut buf)?;
    let c = buf[0] as char;
    match c {
        'C' => Ok((true, c)),
        'P' => Ok((true, c)),
        _ => Ok((false, c)),
    }
}

fn escape_2_uart(port: &mut TTYPort, duration: Duration) -> Result<()> {
    let buf: [u8; 1] = [27];
    const INTERVAL: Duration = Duration::from_millis(10);
    let count = duration.as_micros() / INTERVAL.as_micros();
    for _ in 0..count {
        port.write(&buf)?;
        port.flush()?;
        std::thread::sleep(INTERVAL);
    }
    Ok(())
}

fn chk_magics(port: &mut TTYPort) -> Result<()> {
    const MAX_COUNT: usize = 5;
    const MAX_FAILED_COUNT: usize = 100;
    let mut count = 0;
    let mut failed_count = 0;
    loop {
        let (ok, c) = chk_magic(port)?;
        if ok {
            failed_count = 0;
            count += 1;
            if count == MAX_COUNT {
                break;
            }
        } else {
            failed_count += 1;
            count = 0;
            dbg!(c);
            if failed_count == MAX_FAILED_COUNT {
                bail!(format!(
                    "Failed to check magic: exceeded max failed count ({})",
                    MAX_FAILED_COUNT
                ))
            }
            escape_2_uart(port, Duration::from_millis(30))?;
        }
    }
    Ok(())
}

fn query_mac(port: &mut TTYPort) -> Result<[u8; 6]> {
    // https://github.com/rust-lang/rust/issues/85077
    const QUERY: [u8; 9] = [0x21, 0x06, 0x00, 0xea, 0x2d, 0x38, 0x00, 0x00, 0x00];
    let mut result: [u8; 6] = [0; 6];
    let mut result_buf: [u8; 24] = [0; 24];
    port.clear(ClearBuffer::Input)?;
    port.write(&QUERY)?;
    port.flush()?;
    port.read(&mut result_buf)?;
    // Mac:FFFFFFFFFFFF
    let result_str = String::from_utf8_lossy(&result_buf);
    let mac_str = &result_str[4..17];
    for i in 0..6 {
        let s = &mac_str[i * 2..i * 2 + 2];
        let v = u8::from_str_radix(s, 16)?;
        result[i] = v;
    }
    Ok(result)
}

fn erase_image(port: &mut TTYPort) -> Result<()> {
    const ERASE: [u8; 13] = [
        0x21, 0x0a, 0x00, 0xc3, 0x35, 0x32, 0x00, 0x00, 0x00, 0x02, 0x00, 0xfe, 0x01,
    ];
    port.write(&ERASE)?;
    port.flush()?;
    Ok(())
}

fn set_download_speed(port: &mut TTYPort, speed: u32) -> Result<()> {
    let command: [u8; 13] = match speed {
        115_200 => [
            0x21, 0x0a, 0x00, 0x97, 0x4b, 0x31, 0x00, 0x00, 0x00, 0x00, 0xc2, 0x01, 0x00,
        ],
        460_800 => [
            0x21, 0x0a, 0x00, 0x07, 0x00, 0x31, 0x00, 0x00, 0x00, 0x00, 0x08, 0x07, 0x00,
        ],
        921_600 => [
            0x21, 0x0a, 0x00, 0x5d, 0x50, 0x31, 0x00, 0x00, 0x00, 0x00, 0x10, 0x0e, 0x00,
        ],
        1_000_000 => [
            0x21, 0x0a, 0x00, 0x5e, 0x3d, 0x31, 0x00, 0x00, 0x00, 0x40, 0x42, 0x0f, 0x00,
        ],
        2_000_000 => [
            0x21, 0x0a, 0x00, 0xef, 0x2a, 0x31, 0x00, 0x00, 0x00, 0x80, 0x84, 0x1e, 0x00,
        ],
        _ => {
            return Err(anyhow!(format!(
                "Unsupported speed: {}; available: 115200, 460800, 921600, 1000000, 2000000",
                speed
            )))
        }
    };
    port.write(&command)?;
    port.flush()?;
    // a pause is needed after changing the baud rate
    std::thread::sleep(Duration::from_millis(500));
    port.set_baud_rate(speed)?;
    Ok(())
}

// the original implementation uses 0x1021 as the polynomial
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;

    for &byte in data {
        crc ^= (byte as u16) << 8;

        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }

    crc & 0xFFFF
}

fn generate_frame<T: Read>(
    reader: &mut T,
    pack_counter: u8,
) -> Result<([u8; XMODEM_FRAME_SIZE], bool)> {
    let mut frame_data: [u8; XMODEM_FRAME_SIZE] = [0; XMODEM_FRAME_SIZE];
    let crc: u16;
    let mut eof = false;
    {
        let packet_data =
            &mut frame_data[XMODEM_HEADER_SIZE..XMODEM_HEADER_SIZE + XMODEM_DATA_SIZE];
        assert_eq!(packet_data.len(), XMODEM_DATA_SIZE);
        let sz = reader.read(packet_data)?;
        if sz != XMODEM_DATA_SIZE {
            eof = true;
        }
    }
    {
        let header = &mut frame_data[..XMODEM_HEADER_SIZE];
        header[0] = XMODEM_HEAD;
        header[1] = pack_counter;
        header[2] = 255 - pack_counter;
    }
    {
        let data = &frame_data[XMODEM_HEADER_SIZE..XMODEM_HEADER_SIZE + XMODEM_DATA_SIZE];
        assert_eq!(data.len(), XMODEM_DATA_SIZE);
        crc = crc16(data);
    }
    {
        let tail = &mut frame_data[XMODEM_HEADER_SIZE + XMODEM_DATA_SIZE..];
        tail[0] = (crc >> 8) as u8;
        tail[1] = (crc & 0xff) as u8;
    }
    Ok((frame_data, eof))
}

enum PacketId {
    Ack,
    Nak,
}

fn try_write(port: &mut TTYPort, data: &[u8]) -> Result<PacketId> {
    let mut buf: [u8; 1] = [0];
    port.clear(ClearBuffer::Input)?;
    port.write(data)?;
    port.flush()?;
    port.read_exact(&mut buf)?;

    let id = buf[0];
    match id {
        XMODEM_ACK => Ok(PacketId::Ack),
        XMODEM_NAK => Ok(PacketId::Nak),
        _ => bail!(format!("Received unknown xmodem magic: {:02X}", id)),
    }
}

fn write_image<T: Read>(port: &mut TTYPort, reader: &mut T) -> Result<()> {
    let mut pack_counter: u8 = 1;
    let write_frame = |port: &mut TTYPort, frame: &[u8]| -> Result<()> {
        const MAX_RETRY: usize = 10;
        let mut retry = 0;
        loop {
            let id = try_write(port, frame)?;
            match id {
                PacketId::Ack => {
                    break;
                }
                PacketId::Nak => {
                    retry += 1;
                    if retry == MAX_RETRY {
                        bail!(format!(
                            "Failed to write image: exceeded max retry count ({})",
                            MAX_RETRY
                        ));
                    }
                }
            }
        }
        Ok(())
    };
    loop {
        let (frame, eof) = generate_frame(reader, pack_counter)?;
        write_frame(port, &frame)?;
        pack_counter = pack_counter.wrapping_add(1);
        println!("Pack: {}", pack_counter);
        if eof {
            break;
        }
    }

    loop {
        let buf: [u8; 1] = [XMODEM_EOT];
        port.write(&buf)?;
        let mut buf: [u8; 1] = [0];
        port.read_exact(&mut buf)?;
        let c = buf[0];
        if c == XMODEM_ACK {
            break;
        } else {
            dbg!(c);
        }
    }

    Ok(())
}

fn main() {
    let mut port: TTYPort = serialport::new("/dev/ttyUSB0", 115_200)
        .timeout(Duration::from_millis(30_000))
        .open_native()
        .expect("Failed to open port");
    try_reset(&mut port).expect("Failed to reset");
    escape_2_uart(&mut port, Duration::from_millis(500)).expect("Failed to escape");
    chk_magics(&mut port).expect("Failed to check magic");
    let mac = query_mac(&mut port).expect("Failed to query mac");
    println!(
        "MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let image_path = "/home/crosstyan/Code/wm806-cmake/build/demo.fls";
    let mut image_file = File::open(image_path).expect("Failed to open image file");
    // erase_image(&mut port).expect("Failed to erase image");
    set_download_speed(&mut port, 2000000).expect("Failed to set download speed");
    // set_download_speed(&mut port, 115200).expect("Failed to set download speed");
    write_image(&mut port, &mut image_file).expect("Failed to write image");
    try_reset(&mut port).expect("Failed to reset");
    println!("Done");
}
