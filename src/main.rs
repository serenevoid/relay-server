mod controller;
use controller::SerialController;

fn main() {
    let port = SerialController::open_port();
    let data_to_send = b"101\n";
}
