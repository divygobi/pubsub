use tokio::sync::broadcast;
use tokio::sync::Mutex;

use tonic::client;
use tonic::{transport::Server, Request, Response, Status};

use server::pub_sub_server::{PubSub, PubSubServer};

use server::{SubscribeTopicResponse, SubscribeTopicRequest, 
    UnsubscribeTopicResponse, UnsubscribeTopicRequest, 
    PublishMessageResponse, PublishMessageRequest,
    ClientIdResponse, ClientIdRequest};

pub mod server {
    tonic::include_proto!("pubster"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct Broker {
    //TODO implement ring buffer for avialable client ids.
    next_available_client_id: Mutex<i32>,
    subscribers: std::collections::HashMap<String, std::collections::HashSet<i32>>,
    
}

#[tonic::async_trait]
impl PubSub for Broker{

    async fn publish(&self, request: Request<PublishMessageRequest>) 
    -> Result<Response<PublishMessageResponse>, Status>{
        println!("Got a publish request: {:?}", &request);

        let res = PublishMessageResponse {
            ack: format!("Hello goon your message for topic {} has been recieved", request.into_inner().topic.unwrap().topic_name),
        };

        Ok(Response::new(res))
    }

    //TODO Write this such that if the client is already subscribed, give some sort of Error
    //TODO This needs to return a stream 
    async fn subscribe(&self, request: Request<SubscribeTopicRequest>) 
    -> Result<Response<SubscribeTopicResponse>, Status>{
        println!("Got a subscribe request: {:?}", request);

        let sub_request = request.into_inner();
        let client_id: i32 = sub_request.client_id;
        let topic_name: String = sub_request.topic.unwrap().topic_name;

        // if !self.subscribers.contains_key(&topic_name){
        //     self.subscribers.insert(topic_name.to_string(), std::collections::HashSet::new());
        // }

        // self.subscribers.get_mut(&topic_name).unwrap().insert(client_id);

        let res = SubscribeTopicResponse {
            ack: format!("Hello client your subscription for topic {} has been recieved", topic_name),
        };

        Ok(Response::new(res))
    }

    async fn unsubscribe(&self, request: Request<UnsubscribeTopicRequest>) 
    -> Result<Response<UnsubscribeTopicResponse>, Status>{
        println!("Got a unsubscribe request: {:?}", request);

        let unsub_request = request.into_inner();
        let client_id: i32 = unsub_request.client_id;
        let topic_name: String = unsub_request.topic.unwrap().topic_name;

        // if self.subscribers.contains_key(&topic_name){
        //     self.subscribers.get_mut(&topic_name).unwrap().remove(&client_id);
        // }
        
        let res = UnsubscribeTopicResponse{
            ack: format!("Hello goon your unsubscription for topic {} has been recieved", topic_name),
        };

        Ok(Response::new(res))
    }

    async fn get_client_id(&self, request: Request<ClientIdRequest>) -> Result<Response<ClientIdResponse>, Status> {


        let mut guard = self.next_available_client_id.lock().await;
        *guard += 1;


        let res = Response::new(ClientIdResponse{
            client_id: *guard,
        });


        return Ok(res);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    let addr = "[::1]:50051".parse()?;
    let mut broker = Broker::default();
    Server::builder()
        .add_service(PubSubServer::new(broker))
        .serve(addr)
        .await?;
    
    Ok(())

}