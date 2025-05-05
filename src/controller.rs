use std::{thread, time::Duration};
use serialport::{available_ports, Result, SerialPort};
use std::io::{self, Read, Write};

pub const BAUD_RATE: u32 = 9600;

pub struct SerialController {
    port: Box<dyn SerialPort>,
}

impl SerialController {
    pub fn open_port() -> Result<Self> {
        let serial_result = serialport::new(get_port_address(), BAUD_RATE)
            .timeout(Duration::from_secs(2))
            .open();

        match serial_result {
            Ok(port) => {
                return Ok(SerialController {port} );
            },
            Err(e) => {
                panic!("Failed to open port: {}", e);
            }
        };
    }

    pub fn write(&mut self, data: u16) -> Result<u16, io::Error> {
        let message = format!("{}\n", data);

        if let Err(e) = self.port.write_all(message.as_bytes()) {
            eprintln!("Failed to write to port: {}", e);
            return Err(e);
        }

        thread::sleep(Duration::from_secs(2)); // Allow time for Arduino to respond

        let mut buffer = [0u8; 128];
        let bytes_read = match self.port.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Failed to read from port: {}", e);
                return Err(e);
            }
        };

        let received = &buffer[..bytes_read];
        let text = String::from_utf8_lossy(received);

        match text.trim().parse::<u16>() {
            Ok(num) => Ok(num),
            Err(e) => {
                eprintln!("Failed to parse number from response: {}", e);
                Err(io::Error::new(io::ErrorKind::InvalidData, e))
            }
        }
    }
}

pub fn get_port_address() -> String {
    match available_ports() {
        Ok(ports) => {
            for port in ports {
                if port.port_name.contains("ttyACM") {
                    println!("{}", port.port_name);
                    return String::from(port.port_name);
                }
            }
        },
        Err(err) => {
            panic!("{}", err);
        }
    }
    panic!("Serial device not found.")
}
