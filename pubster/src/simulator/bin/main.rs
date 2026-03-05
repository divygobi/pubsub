// Usage: ./simulator [num_clients] [interval_ms]
// Falls back to NUM_CLIENTS / SPAWN_INTERVAL_MS from .env if args not given.

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

    pubster::logger::init("simulator.log")?;

    let sim = pubster::simulator::Simulator::new(server_url);
    sim.run(num_clients, interval_ms).await;

    // Hold open — clients are still running in spawned tasks.
    tokio::signal::ctrl_c().await?;
    Ok(())
}
