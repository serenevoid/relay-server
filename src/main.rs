mod controller;
mod server;
mod filesystem;

const ADDR: &str = "0.0.0.0:3402";
const CONFIG_PATH: &str = "./data.json";

#[tokio::main]
async fn main() {
    let data = filesystem::load_or_init_json(CONFIG_PATH);
    let mut relay_state: u16 = 0;
    for item in &data.relays {
        if item.state {
            relay_state |= 1 << (item.id - 1);
        }
    };
    controller::set_relays(relay_state).unwrap();

    server::launch_server(data).await;
}
