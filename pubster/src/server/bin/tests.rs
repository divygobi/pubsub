use super::*;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio::time::{sleep, Duration};
use pubster::client::Client;

// ── Helpers ──────────────────────────────────────────────────────────────────

// Spin up a real gRPC server on a random OS-assigned port and return its URL.
// Each test gets an isolated broker with no shared state.
async fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        Server::builder()
            .add_service(PubSubServer::new(BrokerService { broker: Arc::new(Broker::new()) }))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    sleep(Duration::from_millis(50)).await; // let the server bind
    format!("http://127.0.0.1:{}", port)
}

// Collects all ServerEvents currently waiting in the channel without blocking.
fn drain(rx: &mut mpsc::Receiver<ServerEvent>) -> Vec<ServerEvent> {
    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    events
}

fn make_event(payload: &str) -> ServerEvent {
    ServerEvent {
        kind: Some(server_event::Kind::Message(MessageEvent {
            message_id: 0,
            topic_name: "test".to_string(),
            publisher_name: "pub".to_string(),
            payload: payload.to_string(),
        })),
    }
}

// ── ClientQueue unit tests ────────────────────────────────────────────────────

// A fresh queue starts empty with zero drops.
#[test]
fn queue_starts_empty() {
    let q = ClientQueue::new();
    assert_eq!(q.total_dropped, 0);
    assert!(q.messages.is_empty());
}

// Pushing fewer than MAX_DROPPED_QUEUE messages keeps all of them.
#[test]
fn queue_within_cap_retains_all() {
    let mut q = ClientQueue::new();
    for i in 0..10 {
        q.push(make_event(&format!("msg{}", i)));
    }
    assert_eq!(q.total_dropped, 10);
    assert_eq!(q.messages.len(), 10);
    // No overflow — every dropped message is still in the queue.
    assert_eq!(q.total_dropped - q.messages.len(), 0);
}

// Pushing more than MAX_DROPPED_QUEUE evicts the oldest entries.
#[test]
fn queue_over_cap_evicts_oldest() {
    let mut q = ClientQueue::new();
    for i in 0..(MAX_DROPPED_QUEUE + 50) {
        q.push(make_event(&format!("msg{}", i)));
    }
    // total_dropped counts every push regardless of eviction.
    assert_eq!(q.total_dropped, MAX_DROPPED_QUEUE + 50);
    // The in-memory queue never exceeds the cap.
    assert_eq!(q.messages.len(), MAX_DROPPED_QUEUE);
    // The 50 oldest were evicted.
    let lost = q.total_dropped - q.messages.len();
    assert_eq!(lost, 50);
}

// The oldest message is evicted first (FIFO).
#[test]
fn queue_evicts_oldest_first() {
    let mut q = ClientQueue::new();
    for i in 0..=MAX_DROPPED_QUEUE {
        q.push(make_event(&format!("msg{}", i)));
    }
    // msg0 was evicted; the front of the queue should now be msg1.
    let front = q.messages.front().unwrap();
    let payload = match &front.kind {
        Some(server_event::Kind::Message(m)) => &m.payload,
        _ => panic!("expected MessageEvent"),
    };
    assert_eq!(payload, "msg1");
}

// ── Integration tests ─────────────────────────────────────────────────────────

// Publisher sends N messages to a topic and two subscribers both receive all of them.
#[tokio::test]
async fn test_basic_fanout() {
    let url = start_test_server().await;

    let (sub1, mut rx1) = Client::connect_with_receiver(&url, "sub1").await.unwrap();
    let (sub2, mut rx2) = Client::connect_with_receiver(&url, "sub2").await.unwrap();
    sub1.subscribe("sport").await.unwrap();
    sub2.subscribe("sport").await.unwrap();
    sleep(Duration::from_millis(50)).await; // let subscriptions register

    let pub_client = Client::connect(&url, "pub1").await.unwrap();
    for i in 1..=3u32 {
        pub_client.publish("sport", &format!("msg{}", i)).await.unwrap();
    }
    sleep(Duration::from_millis(200)).await;

    let payloads = |events: Vec<ServerEvent>| -> Vec<String> {
        events.into_iter().filter_map(|e| match e.kind {
            Some(server_event::Kind::Message(m)) => Some(m.payload),
            _ => None,
        }).collect()
    };

    assert_eq!(payloads(drain(&mut rx1)), vec!["msg1", "msg2", "msg3"]);
    assert_eq!(payloads(drain(&mut rx2)), vec!["msg1", "msg2", "msg3"]);
}

// Subscriber disconnects, publisher sends messages, subscriber reconnects and
// receives a drop-count error followed by all queued messages in order.
#[tokio::test]
async fn test_dropped_message_queue() {
    let url = start_test_server().await;

    // Connect, subscribe, then disconnect.
    let (sub, _rx) = Client::connect_with_receiver(&url, "sub1").await.unwrap();
    sub.subscribe("news").await.unwrap();
    sleep(Duration::from_millis(50)).await;
    drop(sub);
    drop(_rx);
    sleep(Duration::from_millis(150)).await; // let server process the disconnect

    // Publish 3 messages while the subscriber is gone.
    let pub_client = Client::connect(&url, "pub1").await.unwrap();
    for i in 1..=3u32 {
        pub_client.publish("news", &format!("queued{}", i)).await.unwrap();
    }
    sleep(Duration::from_millis(100)).await;

    // Reconnect with the same name — should trigger queue flush.
    let (_sub2, mut rx2) = Client::connect_with_receiver(&url, "sub1").await.unwrap();
    sleep(Duration::from_millis(200)).await;

    let events = drain(&mut rx2);
    assert!(!events.is_empty(), "expected events on reconnect, got none");

    // First event must be the drop-count error.
    match &events[0].kind {
        Some(server_event::Kind::Error(e)) => {
            assert!(
                e.message.contains("3"),
                "error should mention 3 queued messages, got: {}", e.message
            );
        }
        other => panic!("expected ErrorEvent first, got {:?}", other),
    }

    // Remaining events are the 3 queued messages in order.
    let payloads: Vec<_> = events[1..].iter().filter_map(|e| match &e.kind {
        Some(server_event::Kind::Message(m)) => Some(m.payload.clone()),
        _ => None,
    }).collect();
    assert_eq!(payloads, vec!["queued1", "queued2", "queued3"]);
}
