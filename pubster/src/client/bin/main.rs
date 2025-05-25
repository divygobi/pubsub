
use std::fmt::Write;

use client::pub_sub_client::PubSubClient;

use client::{SubscribeTopicResponse, SubscribeTopicRequest, 
    UnsubscribeTopicResponse, UnsubscribeTopicRequest, 
    PublishMessageResponse, PublishMessageRequest, Topic, Message};

pub mod client {
    tonic::include_proto!("pubster");
}

 #[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = PubSubClient::connect("http://[::1]:50051").await?;
    let pau_theory_topic: Option<Topic> =  Some(Topic{
            topic_name: "Pau Theory".into(),
        });

    let client_id = client.get_id(tonic::Request::new("Meow"));

    let sub_request = tonic::Request::new(SubscribeTopicRequest{
        topic: pau_theory_topic.clone(),
        client_id: 69,
    });

    let sub_response = client.subscribe(sub_request).await?;
    
    if sub_response.into_inner().ack != None{
        tokio::spawn(future)
    }

    let pub_request = tonic::Request::new(PublishMessageRequest{
        message: Some(Message{
            message: String::from("Pau evolves into Gao"),
            topic: pau_theory_topic.clone(),
        }),
        topic: pau_theory_topic.clone(),
        client_id: 69,
    });

    let pub_response = client.publish(pub_request).await?;


    let unsub_request = tonic::Request::new(UnsubscribeTopicRequest{
        topic: pau_theory_topic.clone(),
        client_id: 69,
    });

    let unsub_response = client.unsubscribe(unsub_request).await?;

    

    println!("SUB RESPONSE={:?}", sub_response);
    println!("PUB RESPONSE={:?}", pub_response);
    println!("UNSUB RESPONSE={:?}", unsub_response);

    Ok(())
}