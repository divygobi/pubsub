use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

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

    pub async fn run(&self, num_clients: u32, interval_ms: u64) -> std::io::Result<Vec<Child>> {
        let mut children = Vec::new();
        for _ in 0..num_clients {
            children.push(self.spawn_client()?);
            if interval_ms > 0 {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        }
        Ok(children)
    }

    pub fn spawn_client(&self) -> std::io::Result<Child> {
        let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        let client_name = format!("client{}", id);

        let mut child = Command::new("./target/debug/client")
            .env("SERVER_URL", &self.server_url)
            .stdin(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            writeln!(stdin, "{}", client_name)?;
        }

        Ok(child)
    }
}
