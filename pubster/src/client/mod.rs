use tonic::transport::Channel;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::proto::pub_sub_client::PubSubClient;
use crate::proto::{
    ClientEvent, client_event::Payload,
    ConnectCmd, SubscribeCmd, UnsubscribeCmd, PublishCmd,
    ListTopicsRequest, server_event,
};

pub struct Client {
    grpc: PubSubClient<Channel>,   // for unary RPCs (list_topics)
    tx: mpsc::Sender<ClientEvent>, // for streaming commands
    pub name: String,
}

impl Client {
    pub async fn connect(
        server_url: impl Into<String>,
        client_name: impl Into<String>,
    ) -> Result<Self, crate::ThreadSafeError> {
        let name = client_name.into();
        let mut grpc = PubSubClient::connect(server_url.into()).await?;
        let (tx, rx) = mpsc::channel::<ClientEvent>(32);

        // ConnectCmd must be the first message on the stream
        tx.send(ClientEvent {
            payload: Some(Payload::Connect(ConnectCmd {
                client_name: name.clone(),
            })),
        }).await?;

        let mut in_stream = grpc.handshake(ReceiverStream::new(rx)).await?.into_inner();

        // Background task: print every incoming ServerEvent
        let display_name = name.clone();
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(event) => match event.kind {
                        Some(server_event::Kind::Message(msg)) => crate::log_line!(
                            "[{}][{}] {}: {}",
                            display_name, msg.topic_name, msg.publisher_name, msg.payload
                        ),
                        Some(server_event::Kind::Error(err)) => {
                            eprintln!("[{}] Server error: {}", display_name, err.message);
                            break;
                        }
                        None => {}
                    },
                    Err(e) => {
                        eprintln!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Client { grpc, tx, name })
    }

    // &mut self because PubSubClient::list_topics takes &mut self (tonic requirement)
    pub async fn list_topics(&mut self) -> Result<Vec<String>, crate::ThreadSafeError> {
        let response = self.grpc.list_topics(ListTopicsRequest {}).await?.into_inner();
        Ok(response.topic_names)
    }

    pub async fn subscribe(&self, topic: &str) -> Result<(), crate::ThreadSafeError> {
        self.tx.send(ClientEvent {
            payload: Some(Payload::Subscribe(SubscribeCmd {
                topic_name: topic.to_string(),
            })),
        }).await?;
        Ok(())
    }

    pub async fn publish(&self, topic: &str, payload: &str) -> Result<(), crate::ThreadSafeError> {
        self.tx.send(ClientEvent {
            payload: Some(Payload::Publish(PublishCmd {
                topic_name: topic.to_string(),
                payload: payload.to_string(),
            })),
        }).await?;
        Ok(())
    }

    pub async fn unsubscribe(&self, topic: &str) -> Result<(), crate::ThreadSafeError> {
        self.tx.send(ClientEvent {
            payload: Some(Payload::Unsubscribe(UnsubscribeCmd {
                topic_name: topic.to_string(),
            })),
        }).await?;
        Ok(())
    }
}
