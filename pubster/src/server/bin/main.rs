use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;
use std::pin::Pin;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status, Streaming};
use pubster::proto::pub_sub_server::{PubSub, PubSubServer};

use pubster::proto::{
    ClientEvent, client_event::Payload,
    ServerEvent, server_event, MessageEvent, ErrorEvent,
    ListTopicsRequest, ListTopicsResponse,
};

const MPSC_BUF_SIZE: usize = 32;
const MAX_DROPPED_QUEUE: usize = 256;

type ServerEventStream = Pin<Box<dyn Stream<Item = Result<ServerEvent, Status>> + Send>>;
type Sender = mpsc::Sender<Result<ServerEvent, Status>>;

// Holds messages that could not be delivered while a client was disconnected.
// `messages` is capped at MAX_DROPPED_QUEUE (oldest evicted when full).
// `total_dropped` counts every failed send, including ones that didn't fit in the queue.
#[derive(Debug)]
struct ClientQueue {
    messages: VecDeque<ServerEvent>,
    total_dropped: usize,
}

impl ClientQueue {
    fn new() -> Self {
        ClientQueue { messages: VecDeque::new(), total_dropped: 0 }
    }

    fn push(&mut self, event: ServerEvent) {
        self.total_dropped += 1;
        if self.messages.len() >= MAX_DROPPED_QUEUE {
            self.messages.pop_front(); // evict oldest to make room
        }
        self.messages.push_back(event);
    }
}

#[derive(Debug)]
pub struct Broker {
    // topic_name -> (client_name -> Option<Sender>)
    // None means the client was subscribed but is currently disconnected.
    subscribers: RwLock<HashMap<String, HashMap<String, Option<Sender>>>>,
    // client_name -> messages that couldn't be delivered while disconnected
    dropped_messages: RwLock<HashMap<String, ClientQueue>>,
    next_message_id: AtomicU32,
}

impl Broker {
    pub fn new() -> Self {
        Broker {
            subscribers: RwLock::new(HashMap::new()),
            dropped_messages: RwLock::new(HashMap::new()),
            next_message_id: AtomicU32::new(0),
        }
    }
}

struct BrokerService {
    broker: Arc<Broker>,
}

#[tonic::async_trait]
impl PubSub for BrokerService {
    type HandshakeStream = ServerEventStream;

    async fn handshake(
        &self,
        request: Request<Streaming<ClientEvent>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(MPSC_BUF_SIZE);
        let broker = Arc::clone(&self.broker);

        tokio::spawn(async move {
            // First message must be a ConnectCmd — reject anything else.
            let client_name = match in_stream.next().await {
                Some(Ok(ClientEvent { payload: Some(Payload::Connect(cmd)) })) => cmd.client_name,
                _ => {
                    let _ = tx.send(Ok(ServerEvent {
                        kind: Some(server_event::Kind::Error(ErrorEvent {
                            message: "First message must be ConnectCmd".to_string(),
                        })),
                    })).await;
                    return;
                }
            };

            // Reconnect path — flush any messages queued while this client was gone.
            {
                let mut dropped = broker.dropped_messages.write().await;
                if let Some(queue) = dropped.get_mut(&client_name) {
                    if queue.total_dropped > 0 {
                        let queued = queue.messages.len();
                        let lost = queue.total_dropped - queued;
                        let msg = if lost > 0 {
                            format!(
                                "{} messages dropped while disconnected — {} oldest lost, showing last {}",
                                queue.total_dropped, lost, queued
                            )
                        } else {
                            format!("{} messages queued while disconnected", queued)
                        };
                        let _ = tx.send(Ok(ServerEvent {
                            kind: Some(server_event::Kind::Error(ErrorEvent { message: msg })),
                        })).await;
                        while let Some(event) = queue.messages.pop_front() {
                            let _ = tx.send(Ok(event)).await;
                        }
                        queue.total_dropped = 0;
                    }
                }
            }

            // Reconnect path — restore subscriptions by swapping in the new sender.
            {
                let mut guard = broker.subscribers.write().await;
                for topic_map in guard.values_mut() {
                    if let Some(entry) = topic_map.get_mut(&client_name) {
                        *entry = Some(tx.clone());
                    }
                }
            }

            while let Some(event) = in_stream.next().await {
                match event {
                    Ok(ClientEvent { payload: Some(payload) }) => match payload {
                        Payload::Connect(_) => {
                            // Ignore duplicate ConnectCmds — name already set above.
                        }
                        Payload::Subscribe(cmd) => {
                            let mut guard = broker.subscribers.write().await;
                            guard
                                .entry(cmd.topic_name)
                                .or_insert_with(HashMap::new)
                                .insert(client_name.clone(), Some(tx.clone()));
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
                                kind: Some(server_event::Kind::Message(MessageEvent {
                                    message_id,
                                    topic_name: cmd.topic_name.clone(),
                                    publisher_name: client_name.clone(),
                                    payload: cmd.payload,
                                })),
                            };

                            // Collect (client_name, sender_option) before awaiting so we
                            // don't hold the read lock across await points.
                            let subscribers: Vec<(String, Option<Sender>)> = {
                                let guard = broker.subscribers.read().await;
                                guard
                                    .get(&cmd.topic_name)
                                    .map(|m| m.iter().map(|(n, s)| (n.clone(), s.clone())).collect())
                                    .unwrap_or_default()
                            };

                            for (sub_name, sender_opt) in subscribers {
                                let failed = match sender_opt {
                                    Some(sender) => sender.send(Ok(event.clone())).await.is_err(),
                                    None => true, // client disconnected — queue it
                                };

                                if failed {
                                    let mut dropped = broker.dropped_messages.write().await;
                                    dropped
                                        .entry(sub_name)
                                        .or_insert_with(ClientQueue::new)
                                        .push(event.clone());
                                }
                            }
                        }
                    }
                    Ok(ClientEvent { payload: None }) => {}
                    Err(e) => {
                        eprintln!("stream error for {client_name}: {e}");
                        break;
                    }
                }
            }

            // Client disconnected — mark all their subscriptions as None rather than
            // removing them, so queued messages can still be associated with each topic
            // and the sender is restored on reconnect.
            let mut guard = broker.subscribers.write().await;
            for topic_map in guard.values_mut() {
                if let Some(entry) = topic_map.get_mut(&client_name) {
                    *entry = None;
                }
            }
        });

        let out_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(out_stream) as Self::HandshakeStream))
    }

    async fn list_topics(
        &self,
        _request: Request<ListTopicsRequest>,
    ) -> Result<Response<ListTopicsResponse>, Status> {
        let guard = self.broker.subscribers.read().await;
        let topic_names = guard.keys().cloned().collect();
        Ok(Response::new(ListTopicsResponse { topic_names }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "::1".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("[{}]:{}", host, port).parse()?;
    let broker = Broker::new();

    println!("Server starting on {}", addr);

    Server::builder()
        .add_service(PubSubServer::new(BrokerService { broker: Arc::new(broker) }))
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c().await.ok();
            println!("\nReceived Ctrl+C, shutting down server...");
        })
        .await?;

    println!("Server shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests;
