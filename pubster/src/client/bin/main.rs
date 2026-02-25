use std::io;
use std::fs::{self, OpenOptions};
use std::io::prelude::*;

use client::pub_sub_client::PubSubClient;

use client::{ClientEvent, ConnectCmd, SubscribeCmd, UnsubscribeCmd, PublishCmd,
    ServerEvent, ListTopicsRequest, ListTopicsResponse};
use tokio_stream::StreamExt;

pub mod client {
    tonic::include_proto!("pubster");
}

 #[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //TODO add api to set server ip
    println!("Hello user, I'm the client! I'm gonna try connecting to the server now.");
    let mut server = PubSubClient::connect("http://[::1]:50051").await?;
    
    let client_id = server.get_client_id(tonic::Request::new(ClientIdRequest{
         hello: String::from("Meow"),
    })).await?.into_inner().client_id;

    println!("{}", format!("Connected successfully!! You client id is {}.",client_id));

    let mut option = String::new();
       
    loop {
        print_options(client_id); // Print options again after processing the input
        option.clear();
        io::stdin().read_line(&mut option).expect("Failed to read option");

        match option.trim() {
            "0" => {
                // List topics
                println!("Listing topics...");
                // Here you would call the server to list topics
            },
            "1" => {
                // Subscribe to topic
                println!("Enter topic name to subscribe:");
                let mut topic_input = String::new();
                io::stdin().read_line(&mut topic_input).expect("Failed to read topic");
                let topic_name = topic_input.trim().to_string();

                println!("Subscribing to topic '{}'...", topic_name);
                let sub_res = server.subscribe(SubscribeTopicRequest{
                    topic: Some(Topic {
                        topic_name: topic_name.clone(),
                    }),
                    client_id: client_id
                }).await?;

                // Get the stream from the response
                let mut stream = sub_res.into_inner();

                // Create directory for this client if it doesn't exist
                let dir_path = format!("testResults/{}", client_id);
                fs::create_dir_all(&dir_path).expect("Failed to create directory");

                let topic_for_task = topic_name.clone();

                // Spawn a background task to process the stream
                tokio::spawn(async move {
                    println!("Listening for messages on topic '{}'...", topic_for_task);
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(msg) => {
                                println!("[{}] Received: {}", topic_for_task, msg.message);

                                // Write to file (append mode)
                                if let Err(e) = append_to_file(&topic_for_task, &msg.message, client_id) {
                                    eprintln!("Failed to write to file: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Stream error: {}", e);
                                break;
                            }
                        }
                    }
                    println!("Stream for topic '{}' ended.", topic_for_task);
                });

                println!("Subscribed to '{}'. Messages will be written to {}/{}.txt",
                         topic_name, dir_path, topic_name);
            },
            "2" => {
                // Publish a message
                println!("Publishing a message...");
                // Here you would call the server to publish a message
            },
            "3" => {
                // Unsubscribe from topic
                println!("Unsubscribing from topic...");
                // Here you would call the server to unsubscribe from a topic
            },
            "4" => {
                // Exit
                println!("Exiting...");
                break ;
            },
            _ => {
                println!("Invalid option, please try again.");
//                print_options(client_id);
            }
        }
    }
    
    Ok(())
}


fn print_options(client_id: u32){
        println!("Hello client, here are your options, I am client: {}", client_id);
        println!("0: List Topics");
        println!("1: Subscribe to topic");
        println!("2: Publish a message");
        println!("3: Unsubscribe from topic");
        println!("4: Exit");
}


fn append_to_file(topic_name: &str, message: &str, client_id: u32) -> io::Result<()> {
    let file_path = format!("testResults/{}/{}.txt", client_id, topic_name);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;
    writeln!(file, "{}", message)?;
    Ok(())
}

fn test(){

    // let mut client = PubSubClient::connect("http://[::1]:50051").await?;
    // let pau_theory_topic: Option<Topic> =  Some(Topic{
    //         topic_name: "Pau Theory".into(),
    //     });


    // let client_id = client.get_client_id(tonic::Request::new(ClientIdRequest{
    //     hello: String::from("Meow"),
    // })).await?.into_inner().client_id;


    // let sub_request = tonic::Request::new(SubscribeTopicRequest{
    //     topic: pau_theory_topic.clone(),
    //     client_id: client_id,
    // });

    // let sub_response = client.subscribe(sub_request).await?;
    
    // // if sub_response.into_inner().ack != None{
    // //     tokio::spawn(future)
    // // }

    // let pub_request = tonic::Request::new(PublishMessageRequest{
    //     message: Some(Message{
    //         message: String::from("Pau evolves into Gao"),
    //         topic: pau_theory_topic.clone(),
    //     }),
    //     topic: pau_theory_topic.clone(),
    //     client_id: client_id,
    // });

    // let pub_response = client.publish(pub_request).await?;


    // let unsub_request = tonic::Request::new(UnsubscribeTopicRequest{
    //     topic: pau_theory_topic.clone(),
    //     client_id: client_id,
    // });

    // let unsub_response = client.unsubscribe(unsub_request).await?;
}
