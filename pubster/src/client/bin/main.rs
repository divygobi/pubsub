
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

    let request = tonic::Request::new(SubscribeTopicRequest{
        topic: Some(Topic{
            topic_name: "Pau Theory".into(),
        }),
        client_id: 69,
    });

    let response = client.subscribe(request).await?;


    let request = tonic::Request::new(PublishMessageRequest{
        topic: Some(Topic{
            topic_name: "Pau Theory".into(),
        }),
        client_id: 69,
    });

    let response = client.subscribe(request).await?;


    let request = tonic::Request::new(UnsubscribeTopicRequest{
        topic: Some(Topic{
            topic_name: "Pau Theory".into(),
        }),
        client_id: 69,
    });

    let response = client.unsubscribe(request).await?;

    

    println!("RESPONSE={:?}", response);

    Ok(())
}