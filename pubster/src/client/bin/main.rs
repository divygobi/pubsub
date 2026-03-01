use std::io;

use client::pub_sub_client::PubSubClient;
use client::{
    ClientEvent, client_event::Payload,
    ConnectCmd, SubscribeCmd, UnsubscribeCmd, PublishCmd,
    ListTopicsRequest,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

pub mod client {
    tonic::include_proto!("pubster");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Enter your client name:");
    let mut client_name = String::new();
    io::stdin().read_line(&mut client_name).expect("Failed to read name");
    let client_name = client_name.trim().to_string();

    println!("Connecting to server...");
    let mut server = PubSubClient::connect("http://[::1]:50051").await?;

    // Channel: tx → we send ClientEvents; rx → tonic sends them to the server
    let (tx, rx) = mpsc::channel::<ClientEvent>(32);

    // ConnectCmd must be the first message on the stream
    tx.send(ClientEvent {
        payload: Some(Payload::Connect(ConnectCmd {
            client_name: client_name.clone(),
        })),
    }).await?;

    // Open the bidi stream; pass rx as the outgoing stream
    let mut in_stream = server.handshake(ReceiverStream::new(rx)).await?.into_inner();

    // Background task: print every ServerEvent that arrives
    tokio::spawn(async move {
        while let Some(result) = in_stream.next().await {
            match result {
                Ok(event) => {
                    println!("[{}] {}: {}", event.topic_name, event.publisher_name, event.payload);
                }
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    break;
                }
            }
        }
    });

    // Menu loop
    loop {
        print_options(&client_name);
        let mut option = String::new();
        let n = io::stdin().read_line(&mut option).expect("Failed to read option");
        if n == 0 { break; } // EOF (e.g. piped input exhausted)

        match option.trim() {
            "0" => {
                let response = server.list_topics(ListTopicsRequest {}).await?.into_inner();
                if response.topic_names.is_empty() {
                    println!("No topics.");
                } else {
                    println!("Topics: {}", response.topic_names.join(", "));
                }
            }
            "1" => {
                println!("Enter topic name to subscribe:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                tx.send(ClientEvent {
                    payload: Some(Payload::Subscribe(SubscribeCmd { topic_name: topic.trim().to_string() })),
                }).await?;
            }
            "2" => {
                println!("Enter topic name:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                println!("Enter message:");
                let mut msg = String::new();
                io::stdin().read_line(&mut msg).expect("Failed to read message");
                tx.send(ClientEvent {
                    payload: Some(Payload::Publish(PublishCmd {
                        topic_name: topic.trim().to_string(),
                        payload: msg.trim().to_string(),
                    })),
                }).await?;
            }
            "3" => {
                println!("Enter topic name to unsubscribe:");
                let mut topic = String::new();
                io::stdin().read_line(&mut topic).expect("Failed to read topic");
                tx.send(ClientEvent {
                    payload: Some(Payload::Unsubscribe(UnsubscribeCmd { topic_name: topic.trim().to_string() })),
                }).await?;
            }
            "4" => {
                println!("Exiting...");
                break;
            }
            _ => {
                println!("Invalid option.");
            }
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
