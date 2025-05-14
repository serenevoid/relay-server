use std::{
    fs, path::Path, time
};
use crate::server::{States, Item};

pub fn load_or_init_json(path: &str) -> States {
    if Path::new(path).exists() {
        let contents = fs::read_to_string(path).unwrap_or_else(|_| "{}".into());
        serde_json::from_str(&contents).unwrap_or_else(|_| States { relays: vec![] })
    } else {
        States { relays: (1..=10)
            .map(|i| Item {
                id: i,
                name: String::from("-"),
                panel_category: String::from("-"),
                ipv4: String::from("-.-.-.-"),
                last_updated: time::SystemTime::now(),
                state: false
            })
            .collect()
        }
    }
}

pub fn save_json(data: &States, path: &str) -> Result<(), std::io::Error> {
    fs::write(path, serde_json::to_string_pretty(data).unwrap())
}
