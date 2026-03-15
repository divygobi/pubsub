use tonic::transport::Channel;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::proto::pub_sub_client::PubSubClient;
use crate::proto::{
    ClientEvent, client_event::Payload,
    ConnectCmd, SubscribeCmd, UnsubscribeCmd, PublishCmd,
    ServerEvent, ListTopicsRequest, server_event,
};

pub struct Client {
    grpc: PubSubClient<Channel>,   // for unary RPCs (list_topics)
    tx: mpsc::Sender<ClientEvent>, // for streaming commands
    pub name: String,
}

impl Client {
    // Internal constructor. When `event_tx` is Some, every received ServerEvent is
    // forwarded to that channel in addition to being logged — used by connect_with_receiver.
    async fn connect_inner(
        server_url: impl Into<String>,
        client_name: impl Into<String>,
        event_tx: Option<mpsc::Sender<ServerEvent>>,
    ) -> Result<Self, crate::ThreadSafeError> {
        let name = client_name.into();
        let mut grpc = PubSubClient::connect(server_url.into()).await?;
        let (tx, rx) = mpsc::channel::<ClientEvent>(32);

        // Enqueue ConnectCmd before calling handshake. Safe because the channel has
        // capacity 32 — the message is buffered here and consumed when handshake opens
        // the stream. With a zero-capacity channel this would deadlock.
        tx.send(ClientEvent {
            payload: Some(Payload::Connect(ConnectCmd {
                client_name: name.clone(),
            })),
        }).await?;

        let mut in_stream = grpc.handshake(ReceiverStream::new(rx)).await?.into_inner();

        let display_name = name.clone();
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(event) => match event.kind {
                        Some(server_event::Kind::Message(msg)) => {
                            crate::log_line!(
                                "[{}][{}] {}: {}",
                                display_name, msg.topic_name, msg.publisher_name, msg.payload
                            );
                            if let Some(ref tx) = event_tx {
                                let _ = tx.send(ServerEvent {
                                    kind: Some(server_event::Kind::Message(msg)),
                                }).await;
                            }
                        }
                        Some(server_event::Kind::Error(err)) => {
                            eprintln!("[{}] Server error: {}", display_name, err.message);
                            if let Some(ref tx) = event_tx {
                                let _ = tx.send(ServerEvent {
                                    kind: Some(server_event::Kind::Error(err)),
                                }).await;
                            }
                            // Don't break — the server closes the stream for fatal errors,
                            // so the loop will exit naturally via None. For notifications
                            // (e.g. reconnect queue flush) the stream continues normally.
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

    pub async fn connect(
        server_url: impl Into<String>,
        client_name: impl Into<String>,
    ) -> Result<Self, crate::ThreadSafeError> {
        Self::connect_inner(server_url, client_name, None).await
    }

    // Like connect, but also returns a Receiver that yields every ServerEvent the
    // client receives. Useful for tests that need to assert on received messages
    // without reading a log file.
    pub async fn connect_with_receiver(
        server_url: impl Into<String>,
        client_name: impl Into<String>,
    ) -> Result<(Self, mpsc::Receiver<ServerEvent>), crate::ThreadSafeError> {
        let (event_tx, event_rx) = mpsc::channel(64);
        let client = Self::connect_inner(server_url, client_name, Some(event_tx)).await?;
        Ok((client, event_rx))
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
