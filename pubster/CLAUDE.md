# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Pubster is a Rust-based pub/sub (publish-subscribe) messaging system using gRPC. It implements a message broker where clients can subscribe to topics, publish messages, and receive streamed messages.

## Build Commands

```bash
cargo build                 # Debug build
cargo build --release       # Release build
cargo run --bin server      # Run the broker server (listens on [::1]:50051)
cargo run --bin client      # Run the interactive CLI client
cargo test                  # Run tests (currently minimal)
```

## Architecture

### Core Components

**Broker** (`src/server/bin/main.rs`): The central message broker that:
- Manages client IDs via `AtomicU32` counter
- Tracks topic subscriptions in `RwLock<HashMap<String, HashSet<u32>>>`
- Broadcasts messages via `tokio::sync::broadcast` channel
- Filters streamed messages so clients only receive messages for their subscribed topics

**Client** (`src/client/bin/main.rs`): Interactive CLI with menu-driven interface for subscribing, publishing, and unsubscribing from topics. Writes received messages to `testResults/{client_id}/{topic}.txt`.

### gRPC Service

Defined in `proto/control/control.proto`:
- `get_client_id`: Allocates unique client ID
- `subscribe`: Returns a stream of messages for a topic
- `publish`: Sends a message to a topic
- `unsubscribe`: Removes subscription

### Build-time Code Generation

`build.rs` compiles protobuf files from `proto/` directory using `tonic-build`, generating Rust types and service traits.

## Key Patterns

- Server uses `BroadcastStream` with `filter_map` to deliver topic-specific messages to subscribers
- Thread-safe state via `RwLock` for subscriber map and `AtomicU32` for ID generation
- Streaming responses use `Pin<Box<dyn Stream<...> + Send>>`
