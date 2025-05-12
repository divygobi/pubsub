use std::arch::aarch64::int32x2x2_t;

use tonic::{transport::Server, Request, Response, Status};

use pubster::pub_sub_server::{PubSub, PubSubServer};

use pubster::{subscribe_topic_response, subscribe_topic_request, 
    unsubscribe_topic_response, unsubscribe_topic_request, 
    publish_message_response, publish_message_request};

pub mod server {
    tonic_build::include_proto!("pubster"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct Broker {
    subscribers: std::collections::HashMap<String, [i32]>,
}

#[tonic::async_trait]
impl PubSub for Broker{

    async fn recv_publish(&self, request: Request<publish_message_request>) 
    -> Result<Response<publish_message_response>, Status>{
        println!("Got a publish request: {:?}", request);

        let res = publish_message_response {
            ack: format!("Hello goon your message for topic {} has been recieved", request.into_inner().topic.unwrap().topic_name),
        };

        Ok(Response::new(res))
    }

    async fn recv_subscribe(&self, request: Request<subscribe_topic_request>) 
    -> Result<Response<subscribe_topic_response>, Status>{
        println!("Got a subscribe request: {:?}", request);

        let res = subscribe_topic_response {
            ack: format!("Hello goon your subscription for topic {} has been recieved", request.into_inner().topic.unwrap().topic_name),
        };

        Ok(Response::new(res))
    }

    async fn revc_unsubscribe(&self, request: Request<unsubscribe_topic_request>) 
    -> Result<Response<unsubscribe_topic_response>, Status>{
        println!("Got a unsubscribe request: {:?}", request);

        let res = publish_message_response {
            ack: format!("Hello goon your message for topic {} has been recieved", request.into_inner().topic.unwrap().topic_name),
        };

        Ok(Response::new(res))
    }
}

fn main(){

}