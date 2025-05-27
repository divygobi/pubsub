use std::io;
use std::fmt::Write;

use client::pub_sub_client::PubSubClient;

use client::{SubscribeTopicResponse, SubscribeTopicRequest, 
    UnsubscribeTopicResponse, UnsubscribeTopicRequest, 
    PublishMessageResponse, PublishMessageRequest, ClientIdRequest, ClientIdResponse
    ,Topic, Message};

pub mod client {
    tonic::include_proto!("pubster");
}

 #[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //TODO add api to set server ip
    println!("Hello user, I'm the client! I'm gonna try connecting to the server now.");
    let mut client = PubSubClient::connect("http://[::1]:50051").await?;
    
    let client_id = client.get_client_id(tonic::Request::new(ClientIdRequest{
         hello: String::from("Meow"),
    })).await?.into_inner().client_id;

    println!("{}", format!("Connected successfully!! You client id is {}.",client_id));

    let mut option = String::new();
       
    loop {
        print_options(); // Print options again after processing the input
        io::stdin().read_line(&mut option).expect("Failed to read option");

        match option.trim() {
            "0" => {
                // List topics
                println!("Listing topics...");
                // Here you would call the server to list topics
            },
            "1" => {
                // Subscribe to topic
                println!("Subscribing to topic...");
                // Here you would call the server to subscribe to a topic
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
                break;
            },
            _ => {
                println!("Invalid option, please try again.");
                print_options();
            }
        }
    }
    
    Ok(())
}


fn print_options(){
        println!("Hello client, here are your options");
        println!("0: List Topics");
        println!("1: Subscribe to topic");
        println!("2: Publish a message");
        println!("3: Unsubscribe from topic");
        println!("4: Exit");
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