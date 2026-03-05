use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::client::Client;

pub struct Simulator {
    server_url: String,
    next_client_id: AtomicU32,
}

impl Simulator {
    pub fn new(server_url: String) -> Self {
        Simulator {
            server_url,
            next_client_id: AtomicU32::new(1),
        }
    }

    pub async fn run(&self, num_clients: u32, interval_ms: u64) {
        for _ in 0..num_clients {
            self.spawn_client().await;
            if interval_ms > 0 {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        }
    }

    pub async fn spawn_client(&self) {
        let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        let client_name = format!("client{}", id);
        let server_url = self.server_url.clone();

        tokio::spawn(async move {
            let client = match Client::connect(server_url, client_name.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("spawn_client error: {}", e);
                    return;
                }
            };
            if let Err(e) = client.subscribe(&client_name).await {
                eprintln!("subscribe error: {}", e);
            }
            // hold the client alive until the process exits
            tokio::signal::ctrl_c().await.ok();
        });
    }
}
