use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::client::Client;

pub enum SelectionMode {
    Random,
    Deterministic,
}

pub struct Simulator {
    server_url: String,
    next_client_id: AtomicU32,
    selection_mode: SelectionMode,
}

impl Simulator {
    pub fn new(server_url: String, selection_mode: SelectionMode) -> Self {
        Simulator {
            server_url,
            next_client_id: AtomicU32::new(1),
            selection_mode,
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
        let use_random = matches!(self.selection_mode, SelectionMode::Random);

        tokio::spawn(async move {
            let mut client = match Client::connect(server_url, client_name).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("spawn_client error: {}", e);
                    return;
                }
            };

            let topics = match client.list_topics().await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("list_topics error: {}", e);
                    vec![]
                }
            };

            let selected = if use_random {
                let time_seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64;
                random_select(&topics, 10, id as u64 ^ time_seed)
            } else {
                deterministic_select(&topics, 10)
            };

            for topic in &selected {
                if let Err(e) = client.subscribe(topic).await {
                    eprintln!("subscribe error: {}", e);
                }
            }

            // hold the client alive until the process exits
            tokio::signal::ctrl_c().await.ok();
        });
    }
}

// Returns the first n items from the slice (or all of them if fewer than n exist).
fn deterministic_select(items: &[String], n: usize) -> Vec<String> {
    items.iter().take(n).cloned().collect()
}

// Fisher-Yates shuffle via LCG, returns up to n items from the slice.
fn random_select(items: &[String], n: usize, seed: u64) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    let mut indices: Vec<usize> = (0..items.len()).collect();
    let mut rng = seed ^ 0x9e3779b97f4a7c15;
    for i in (1..indices.len()).rev() {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }
    indices.into_iter().take(n).map(|i| items[i].clone()).collect()
}
