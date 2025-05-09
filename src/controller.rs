use serialport::{available_ports, Result, SerialPort};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::Duration
};

static mut SERIAL_PORT: Option<Arc<Mutex<Option<Box<dyn SerialPort + Send>>>>> = None;

pub const BAUD_RATE: u32 = 9600;

#[derive(Debug)]
pub struct SerialController {
    port: Box<dyn SerialPort>,
}

impl SerialController {
    pub fn open_port() {
        let serial_result = serialport::new(get_port_address(), BAUD_RATE)
            .timeout(Duration::from_secs(2))
            .open();

        match serial_result {
            Ok(port) => {
                unsafe {
                    SERIAL_PORT = Some(Arc::new(Mutex::new(Some(port))));
                }
            },
            Err(e) => {
                panic!("Failed to open port: {}", e);
            }
        };
    }

    pub fn write(data: u16) -> Result<u16> {
        let message = format!("{}\n", data);

        if let Err(e) = SERIAL_PORT.unwrap().write_all(message.as_bytes()) {
            eprintln!("Failed to write to port: {}", e);
            return Err(serialport::Error { kind: (serialport::ErrorKind::Unknown), description: (String::from("Failed to write to port")) });
        }

        thread::sleep(Duration::from_secs(2));

        let mut buffer = [0u8; 128];
        let bytes_read = match self.port.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Failed to read from port: {}", e);
                return Err(serialport::Error { kind: (serialport::ErrorKind::Unknown), description: (String::from("Failed to read from port")) });
            }
        };

        let received = &buffer[..bytes_read];
        let text = String::from_utf8_lossy(received);

        match text.trim().parse::<u16>() {
            Ok(num) => Ok(num),
            Err(e) => {
                eprintln!("Failed to parse number from response: {}", e);
                return Err(serialport::Error { kind: (serialport::ErrorKind::InvalidInput), description: (String::from("Failed to parse response"))});
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
