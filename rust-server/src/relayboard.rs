use std::{collections::HashSet, net::{IpAddr, Ipv4Addr}, time::Duration};
use reqwest::Client;
use tokio::{net::UdpSocket, time::{timeout, Instant}};
use futures::stream::{FuturesUnordered, StreamExt};
use crate::server::States;

pub struct Board {
    pub ip: Ipv4Addr,
    client: Client,
    socket: UdpSocket
}

impl Board {
    pub async fn new(ip: Ipv4Addr) -> Self {
        Self {
            client: Client::new(),
            ip,
            socket: UdpSocket::bind("0.0.0.0:999").await.expect("Could not initiate UDP socket")
        }
    }

    pub async fn find_ip() -> Ipv4Addr {
        let client = Client::new();
        let base_ip = "10.8.32";

        let mut tasks = FuturesUnordered::new();

        for i in 1..=254 {
            let client = client.clone();
            let ip = format!("{}.{}", base_ip, i);
            let url = format!("http://{}/esp", ip);

            tasks.push(async move {
                match timeout(Duration::from_secs(1), client.get(&url).send()).await {
                    Ok(Ok(resp)) if resp.status().is_success() => {
                        if let Ok(_body) = resp.text().await {
                            Some(ip)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            });
        }

        while let Some(result) = tasks.next().await {
            if let Some(ip_str) = result {
                return ip_str.parse::<Ipv4Addr>().expect("Invalid IP format");
            }
        }

        panic!("No board responding to /esp on subnet {}", base_ip);
    }

    pub async fn set_relay(&self, data: &States) -> Result<u16, tokio::io::Error> {
        let url = format!("http://{}/set", self.ip);
        let mut relay_state: u16 = 0;
        for item in &data.relays {
            if item.state {
                relay_state |= 1 << (item.id - 1);
            }
        };
        let response = self.client
            .post(url)
            .header("Content-type", "text/plain")
            .body(format!("{relay_state}"))
            .send()
            .await;
        if let Ok(content) = response {
            if let Ok(text) = content.text().await {
                if let Ok(state) = text.parse() {
                    Ok(state)
                } else {
                    return Err(tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, "Invalid Content"))
                }
            } else {
                return Err(tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, "Invalid Content"))
            }
        } else {
            return Err(tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, "Invalid Response"))
        }
    }

    pub async fn get_panels(&self) -> Vec<Ipv4Addr> {
        let broadcast_address = "10.8.32.255:991";
        self.socket.set_broadcast(true).expect("Failed to enable broadcast");
        self.socket.send_to(b"WhereAreYou.01", broadcast_address).await
            .expect("Couldn't send UDP packet for board fetching.");

        let mut buf = [0u8; 256];
        let mut seen_ips = HashSet::new();
        let timeout_duration = Duration::from_secs(3);
        let start = Instant::now();

        while Instant::now().duration_since(start) < timeout_duration {
            match timeout(Duration::from_millis(500), self.socket.recv_from(&mut buf)).await {
                Ok(Ok((_len, src))) => {
                    if let IpAddr::V4(ip) = src.ip() {
                        seen_ips.insert(ip);
                    }
                },
                _ => {
                    continue;
                }
            }
        }
        seen_ips.into_iter().collect()
    }
}
