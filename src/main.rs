mod controller;

use std::{thread, time::Duration};

use controller::SerialController;

fn main() {
    let mut port = SerialController::open_port().unwrap();
    thread::sleep(Duration::from_secs(12));
    let response = port.write(101).unwrap();
    println!("{}", response);
}
