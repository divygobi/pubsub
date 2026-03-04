use std::io;
use pubster::client::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Enter your client name:");
    let mut client_name = String::new();
    io::stdin().read_line(&mut client_name).expect("Failed to read name");
    let client_name = client_name.trim().to_string();

    dotenvy::dotenv().ok();
    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| {
        let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "::1".to_string());
        let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "50051".to_string());
        format!("http://[{}]:{}", host, port)
    });

    println!("Connecting to server...");
    let mut client = Client::connect(server_url, client_name).await?;

    loop {
        print_options(&client.name);
        let mut option = String::new();
        let n = io::stdin().read_line(&mut option).expect("Failed to read option");
        if n == 0 { break; }

        match option.trim() {
            "0" => {
                let topics = client.list_topics().await?;
                if topics.is_empty() {
                    println!("No topics.");
                } else {
                    println!("Topics: {}", topics.join(", "));
                }
            }
            "1" => {
                println!("Enter topic name to subscribe:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                client.subscribe(topic.trim()).await?;
            }
            "2" => {
                println!("Enter topic name:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                println!("Enter message:");
                let mut msg = String::new();
                io::stdin().read_line(&mut msg).expect("Failed to read message");
                client.publish(topic.trim(), msg.trim()).await?;
            }
            "3" => {
                println!("Enter topic name to unsubscribe:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                client.unsubscribe(topic.trim()).await?;
            }
            "4" => {
                println!("Exiting...");
                break;
            }
            _ => println!("Invalid option."),
        }
    }

    Ok(())
}

fn print_options(client_name: &str) {
    println!("\nHello {}, here are your options:", client_name);
    println!("0: List Topics");
    println!("1: Subscribe to topic");
    println!("2: Publish a message");
    println!("3: Unsubscribe from topic");
    println!("4: Exit");
}
