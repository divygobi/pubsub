use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::*;
use std::pin::Pin;
use tokio_stream::{Stream};
use tokio_stream::wrappers::BroadcastStream;
use tonic::{transport::Server, Request, Response, Status};
use server::pub_sub_server::{PubSub, PubSubServer};

use server::{SubscribeTopicRequest, 
    UnsubscribeTopicResponse, UnsubscribeTopicRequest, 
    PublishMessageResponse, PublishMessageRequest,
    ClientIdResponse, ClientIdRequest, SubscribeMessage, Topic};


pub mod server {
    tonic::include_proto!("pubster"); // The string specified here must match the proto package name
}

#[derive(Debug)]
pub struct Broker {
    //TODO implement ring buffer for avialable client ids.
    next_available_client_id: AtomicU32,
    next_available_message_id: AtomicU32,
    //locked hashmap that maps topic name to list of client ids
    subscribers: RwLock<std::collections::HashMap<String, std::collections::HashSet<u32>>>,
    tx: broadcast::Sender<SubscribeMessage>,
}



impl Broker {
    pub fn new() -> Self{
        let (tx, _rx) = broadcast::channel(128);
        Broker { next_available_client_id: AtomicU32::new(0), next_available_message_id: AtomicU32::new(0), subscribers: Default::default(), tx: tx }
    }
}


#[tonic::async_trait]
impl PubSub for Broker{
    type subscribeStream = Pin<Box<dyn Stream<Item = Result<SubscribeMessage, Status>> + Send>>;



    async fn publish(&self, request: Request<PublishMessageRequest>) 
    -> Result<tonic::Response<PublishMessageResponse>, Status>{ 
        println!("Got a publish request: {:?}", &request);
        let pub_request = request.into_inner();
        let topic_name: String = pub_request.topic.unwrap().topic_name;
        let message_data: String = pub_request.message.unwrap().message;
        let publish_client_id: u32 = pub_request.client_id;
        
        
        self.next_available_message_id.fetch_add(1, Ordering::Relaxed);

        let message = SubscribeMessage {
            message_id: self.next_available_message_id.load(Ordering::Relaxed),
            message: message_data,
            publish_client_id: publish_client_id,
            topic: Some(Topic { 
                topic_name: topic_name.clone(),
            }),
            ack: Some(String::from("Hello")),
        };

        let _ = self.tx.send(message);
        
        let res = PublishMessageResponse {
            ack: format!("Hello goon your message for topic {} has been recieved", topic_name),
        };

        Ok(Response::new(res))
    }

    //TODO Write this such that if the client is already subscribed, give some sort of Error
    //TODO This needs to return a stream 
    async fn subscribe(&self, request: Request<SubscribeTopicRequest>) 
    -> Result<tonic::Response<Self::subscribeStream>, Status>{ 
        println!("Got a subscribe request: {:?}", request);

        let sub_request = request.into_inner();
        let client_id: u32 = sub_request.client_id;
        let topic_name: String = sub_request.topic.unwrap().topic_name;

        let mut subscribers_guard = self.subscribers.write().await;
        subscribers_guard
            .entry(topic_name.clone())
            .or_default()            // inserts a new HashSet if none exists
            .insert(client_id);      // then inserts the client into it

        let rx = self.tx.subscribe();

        let filtered_rx_stream = BroadcastStream::new(rx)
            .filter_map(move |msg| {
                    match msg {
                        Ok(msg) => {
                            if let Some(topic) = &msg.topic {
                                if topic.topic_name == topic_name{
                                    return Some(Ok(msg));
                                }
                            }
                            None
                        }
                        Err(_) => None
                    }
            });


        Ok(Response::new(
            Box::pin(filtered_rx_stream) as Self::subscribeStream
        ))
    
    }


    async fn unsubscribe(&self, request: Request<UnsubscribeTopicRequest>) 
    -> Result<Response<UnsubscribeTopicResponse>, Status>{
        println!("Got a unsubscribe request: {:?}", request);

        let unsub_request = request.into_inner();
        let client_id: u32 = unsub_request.client_id;
        let topic_name: String = unsub_request.topic.unwrap().topic_name;

        let mut subscribers_guard = self.subscribers.write().await;
        if let Some(set) = subscribers_guard.get_mut(&topic_name) {
            set.remove(&client_id);
        } 

        let res = UnsubscribeTopicResponse{
            ack: format!("Hello goon your unsubscription for topic {} has been recieved", topic_name),
        };

        Ok(Response::new(res))
    }

    async fn get_client_id(&self, request: Request<ClientIdRequest>) -> Result<Response<ClientIdResponse>, Status> {


        self.next_available_client_id.fetch_add(1, Ordering::Relaxed);

        let res = Response::new(ClientIdResponse{
            client_id: self.next_available_client_id.load(Ordering::Relaxed),
        });


        return Ok(res);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    let addr = "[::1]:50051".parse()?;
    let mut broker = Broker::new();
    Server::builder()
        .add_service(PubSubServer::new(broker))
        .serve(addr)
        .await?;
    
    Ok(())

}