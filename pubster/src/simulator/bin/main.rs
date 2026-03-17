// Usage: ./simulator [num_clients] [interval_ms]
// Falls back to NUM_CLIENTS / SPAWN_INTERVAL_MS from .env if args not given.

use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();

    let num_clients: u32 = args.get(1)
        .map(|s| s.parse().expect("num_clients must be a number"))
        .unwrap_or_else(|| std::env::var("NUM_CLIENTS")
            .unwrap_or_else(|_| "1".to_string())
            .parse().expect("NUM_CLIENTS must be a number"));

    let interval_ms: u64 = args.get(2)
        .map(|s| s.parse().expect("interval_ms must be a number"))
        .unwrap_or_else(|| std::env::var("SPAWN_INTERVAL_MS")
            .unwrap_or_else(|_| "0".to_string())
            .parse().expect("SPAWN_INTERVAL_MS must be a number"));

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "::1".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "50051".to_string());
    let server_url = format!("http://[{}]:{}", host, port);

    let log_path = std::env::var("LOG_PATH")
        .unwrap_or_else(|_| "testResults/simulator.log".to_string());
    let append = std::env::var("LOG_MODE").as_deref() == Ok("append");
    if let Some(parent) = std::path::Path::new(&log_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    pubster::logger::init(&log_path, append)?;

    let mode = match std::env::var("SELECTION_MODE").as_deref() {
        Ok("deterministic") => pubster::simulator::SelectionMode::Deterministic,
        _ => pubster::simulator::SelectionMode::Random,
    };

    let sim = pubster::simulator::Simulator::new(server_url, mode);

    loop {
        println!("\n--- Running simulation ({} clients) ---", num_clients);
        sim.run(num_clients, interval_ms).await;

        print!("\nPress Enter to run again, or Ctrl+C to quit... ");
        io::stdout().flush()?;

        // Run the blocking stdin read on a thread-pool thread so the tokio
        // runtime stays free to handle signals. Race it against Ctrl+C.
        let enter = tokio::task::spawn_blocking(|| {
            let mut buf = String::new();
            io::stdin().read_line(&mut buf)
        });

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down simulator...");
                break;
            }
            result = enter => { result??; }
        }
    }
    Ok(())
}
