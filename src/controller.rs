use serialport::{available_ports, Result, SerialPort};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::Duration
};
use once_cell::sync::Lazy;

use crate::server::States;

static SERIAL_CONTROLLER: Lazy<Arc<Mutex<Option<SerialController>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

pub const BAUD_RATE: u32 = 9600;

pub struct SerialController {
    port: Box<dyn SerialPort + Send>,
}

impl SerialController {
    pub fn open_port() -> Result<Self> {
        let port = serialport::new(get_port_address(), BAUD_RATE)
            .timeout(Duration::from_secs(2))
            .open().unwrap();

        Ok(SerialController { port })
    }

    pub fn write(&mut self, data: u16) -> Result<u16> {
        let message = format!("{}\n", data);

        if let Err(e) = self.port.write_all(message.as_bytes()) {
            eprintln!("Failed to write to port: {}", e);
            return Err(serialport::Error { 
                kind: (serialport::ErrorKind::Unknown),
                description: (String::from("Failed to write to port"))
            });
        }

        thread::sleep(Duration::from_secs(2));

        let mut buffer = [0u8; 128];
        let bytes_read = match self.port.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Failed to read from port: {}", e);
                return Err(serialport::Error { 
                    kind: (serialport::ErrorKind::Unknown),
                    description: (String::from("Failed to read from port"))
                });
            }
        };

        let received = &buffer[..bytes_read];
        let text = String::from_utf8_lossy(received);

        match text.trim().parse::<u16>() {
            Ok(num) => Ok(num),
            Err(e) => {
                eprintln!("Failed to parse number from response: {}", e);
                return Err(serialport::Error {
                    kind: (serialport::ErrorKind::InvalidInput),
                    description: (String::from("Failed to parse response"))
                });
            }
        }
    }
}

fn get_port_address() -> String {
    match available_ports() {
        Ok(ports) => {
            for port in ports {
                if port.port_name.contains("COM8") {
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

pub fn set_relays(data: &States) -> Result<bool> {
    let mut relay_state: u16 = 0;
    for item in &data.relays {
        if item.state {
            relay_state |= 1 << (item.id - 1);
        }
    };
    let mut controller_lock = SERIAL_CONTROLLER.lock().unwrap();

    if controller_lock.is_none() {
        *controller_lock = Some(SerialController::open_port().unwrap());
        thread::sleep(Duration::from_secs(12));
    }

    if let Some(controller) = controller_lock.as_mut() {
        Ok(relay_state == controller.write(relay_state).unwrap())
    } else {
        Err(serialport::Error::new(
            serialport::ErrorKind::Unknown,
            "Serial controller unavailable",
        ))
    }
}
