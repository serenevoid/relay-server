mod controller;
mod server;
mod filesystem;

const ADDR: &str = "0.0.0.0:3402";
const CONFIG_PATH: &str = "./data.json";

#[tokio::main]
async fn main() {
    let data = filesystem::load_or_init_json(CONFIG_PATH);
    controller::set_relays(&data).unwrap();

    server::launch_server(data).await;
}
