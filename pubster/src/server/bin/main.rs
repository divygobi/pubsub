use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::*;
use std::pin::Pin;
use tokio_stream::{Stream};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status, Streaming};
use pubster::proto::pub_sub_server::{PubSub, PubSubServer};

use pubster::proto::{ClientEvent, client_event::Payload, ConnectCmd, SubscribeCmd, UnsubscribeCmd, PublishCmd,
    ServerEvent, ListTopicsRequest, ListTopicsResponse};

const MPSC_BUF_SIZE:usize = 32;
    
type ServerEventStream = Pin<Box<dyn Stream<Item = Result<ServerEvent, Status>> + Send>>;


#[derive(Debug)]
pub struct Broker {
    // topic_name -> (client_name -> sender for that client's outgoing stream)
    // need to clean up dead entries when a client disconnects, could iterate thru topics, we shall
    subscribers: RwLock<HashMap<String, HashMap<String, mpsc::Sender<Result<ServerEvent, Status>>>>>,
    next_message_id: AtomicU32,
}

impl Broker {
    pub fn new() -> Self {
        Broker {
            subscribers: RwLock::new(HashMap::new()),
            next_message_id: AtomicU32::new(0),
        }
    }
}

struct BrokerService(Arc<Broker>);

#[tonic::async_trait]
impl PubSub for BrokerService {
    type HandshakeStream = ServerEventStream;

    async fn handshake(
        &self,
        request: Request<Streaming<ClientEvent>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(MPSC_BUF_SIZE);
        let broker = Arc::clone(&self.0);

        tokio::spawn(async move {
            let mut client_name = String::new();

            while let Some(event) = in_stream.next().await {
                match event {
                    Ok(ClientEvent { payload: Some(payload) }) => match payload {
                        Payload::Connect(cmd) => {
                            client_name = cmd.client_name;
                        }
                        Payload::Subscribe(cmd) => {
                            let mut guard = broker.subscribers.write().await;
                            guard
                                .entry(cmd.topic_name)
                                .or_insert_with(HashMap::new)
                                .insert(client_name.clone(), tx.clone());
                        }
                        Payload::Unsubscribe(cmd) => {
                            let mut guard = broker.subscribers.write().await;
                            if let Some(topic_map) = guard.get_mut(&cmd.topic_name) {
                                topic_map.remove(&client_name);
                            }
                        }
                        Payload::Publish(cmd) => {
                            let message_id = broker.next_message_id.fetch_add(1, Ordering::Relaxed);
                            let event = ServerEvent {
                                message_id,
                                topic_name: cmd.topic_name.clone(),
                                publisher_name: client_name.clone(),
                                payload: cmd.payload,
                            };
                            // collect senders before awaiting so we don't hold the lock across awaits
                            let senders: Vec<_> = {
                                let guard = broker.subscribers.read().await;
                                guard
                                    .get(&cmd.topic_name)
                                    .map(|m| m.values().cloned().collect())
                                    .unwrap_or_default()
                            };
                            for sender in senders {
                                let _ = sender.send(Ok(event.clone())).await;
                            }
                        }
                    }
                    Ok(ClientEvent { payload: None }) => {}
                    Err(_) => break,
                }
            }

            // client disconnected — remove from all topics
            let mut guard = broker.subscribers.write().await;
            for topic_map in guard.values_mut() {
                topic_map.remove(&client_name);
            }
        });

        let out_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(out_stream) as Self::HandshakeStream))
    }

    async fn list_topics(
        &self,
        _request: Request<ListTopicsRequest>,
    ) -> Result<Response<ListTopicsResponse>, Status> {
        let guard = self.0.subscribers.read().await;
        let topic_names = guard.keys().cloned().collect();
        Ok(Response::new(ListTopicsResponse { topic_names }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    dotenvy::dotenv().ok();
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "::1".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("[{}]:{}", host, port).parse()?;
    let broker = Broker::new();

    println!("Server starting on {}", addr);

    Server::builder()
        .add_service(PubSubServer::new(BrokerService(Arc::new(broker))))
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c().await.ok();
            println!("\nReceived Ctrl+C, shutting down server...");
        })
        .await?;

    println!("Server shutdown complete");
    Ok(())
}
