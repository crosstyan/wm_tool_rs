use serialport::{available_ports, SerialPort, TTYPort};
use std::io::Read;
use std::time::Duration;

fn pr(){
    let ps = available_ports().expect("No serial ports found!");
    for p in ps {
        println!("{}", p.port_name);
    }
}

fn print_only(mut port: TTYPort){
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

fn main() {
    let mut port: TTYPort = serialport::new("/dev/ttyUSB0", 115_200)
        .timeout(Duration::from_millis(10))
        .open_native()
        .expect("Failed to open port");
}
