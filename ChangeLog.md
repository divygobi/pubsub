# Pubster

A pub/sub message broker built in Rust, used as a learning vehicle for concurrency primitives,
distributed systems patterns, and systems performance optimization.

---

## Current Status

| Goal | Status |
|------|--------|
| 1. Basic client & server | ✅ Done |
| 2. Simulator | ✅ Done |
| 3. Benchmark & test harness | Planned |
| 4. ACKs + durable sessions | Planned |
| 5. Cross-broker fanout & load balancer | Planned |
| 6. Performance: zero-copy, batching, lock-free | Planned |
| 7. Durable replicated log (WAL + replication + fault tolerance) | Planned |

---

## Goal 1 — Basic client & server ✅

Single gRPC broker using a bidirectional streaming `Handshake` RPC. Clients subscribe,
publish, and unsubscribe over a single persistent stream. Server fans out messages to all
subscribers of a topic.

**Key concurrency primitives used:**
- `tokio::sync::mpsc` per client for outgoing message delivery
- `RwLock<HashMap>` for the subscriber map (read-heavy workload)
- `AtomicU32` for lock-free message ID generation

**Known bottleneck:** the `RwLock` on the subscriber map is contended on every publish,
subscribe, and unsubscribe. This is the first thing to fix in Goal 4.

---

## Goal 2 — Simulator ✅

A `Simulator` struct that spawns in-process `Client` instances (one `tokio::spawn` per
client) with configurable count and spawn interval. Configured via `.env` with CLI arg
overrides.

```bash
cargo run --bin simulator              # uses NUM_CLIENTS=3, SPAWN_INTERVAL_MS=500 from .env
cargo run --bin simulator -- 10 100   # 10 clients, 100ms apart
```

**Implementation note — subprocess → in-process migration:**
The original simulator forked `./target/debug/client` as an OS subprocess and wrote the
client name to its stdin. This was replaced with `Client::connect()` called inside a
`tokio::spawn`, so the simulator manages real `Client` structs in-process.

**Rust lesson encountered — `Send` bounds on `tokio::spawn`:**
`tokio::spawn` requires its future to be `Send` (safe to move across threads). The
initial implementation held the `Result<Client, Box<dyn Error>>` value across a
`.await` point (the `ctrl_c()` wait). Because `Box<dyn Error>` is not `Send`, the
compiler rejected it — even though the `Err` arm had already returned. The fix: extract
`Client` out of the `Result` before the await (early return on error), so only `Client`
(which is `Send`) lives across the suspension point.

---

## Goal 3 — Benchmark & test harness

Every subsequent goal involves a tradeoff. Without measurement you cannot know if a
change helped, hurt, or just moved the bottleneck. The harness must be in place *before*
any optimization work begins.

### Unit tests

Each component should be independently testable without a running server:
- Broker logic: subscribe, publish, unsubscribe, fan-out correctness (use `tokio::test`)
- Session state: durable reconnect, replay, ACK cursor advancement
- WAL (Goal 7): append, replay, compaction

Pattern: construct the `Broker` struct directly in tests, call methods, assert on
channel outputs. No network required.

### Integration tests

Spin up a real server in-process (bind on a random port via `Server::bind` with port `0`),
connect real clients, assert on end-to-end message delivery. This catches serialization
bugs, stream lifecycle issues, and timing problems that unit tests miss.

`tokio::test` with `#[tokio::test(flavor = "multi_thread")]` is enough — no separate
test binary needed.

### Benchmarks (criterion)

Add a `benches/` directory with [criterion](https://github.com/bheisler/criterion.rs)
benchmarks. Each benchmark should measure one thing and record a baseline before any
optimization is applied:

| Benchmark | What it measures |
|---|---|
| `publish_1_sub` | Latency: single publisher → single subscriber |
| `publish_N_subs` | Fan-out cost as subscriber count scales (1, 10, 100, 1000) |
| `subscribe_throughput` | Subscribe/unsubscribe ops per second |
| `concurrent_publishers` | Throughput under N concurrent publishers |

**What to record:** p50, p99, p999 latency; messages/sec throughput; allocations/op
(use `cargo bench` + `perf` or `heaptrack` for allocation profiling).

> Without a recorded baseline, any "optimization" is guesswork. Commit the criterion
> baseline files (`benches/`) so regressions are caught automatically in later goals.

### Load test via simulator

The simulator is already the right shape for load testing — extend it to:
- Report aggregate publish rate and observed receive latency (timestamp in payload,
  measure delta at subscriber)
- Print a summary on Ctrl+C: total messages, p50/p99 latency, drop count

This gives a cheap end-to-end load test without a separate benchmarking framework.

### Flame graphs

For finding *where* CPU time is actually going — the Rust equivalent of C++ flame maps.
Use **`cargo-flamegraph`**, which wraps `perf` (Linux) or `DTrace` (macOS) and produces
the same interactive SVG flame graphs:

```bash
cargo install flamegraph
# run the broker under load, then:
cargo flamegraph --bin server
```

The output is an SVG you open in a browser: wide frames = hot code, tall stacks = deep
call chains. Use this before committing to any micro-optimization in Goal 6 — the flame
graph will tell you if the bottleneck is what you think it is.

> **Roll your own everything, except this.** Criterion and flamegraph are industry-standard
> tools. Using them means your benchmarks and profiles are comparable to real-world Rust
> projects, and you'll be familiar with the same tools used in production systems.

### Milestone checklist

- [ ] `cargo test` passes with unit tests for Broker subscribe/publish/unsubscribe/fanout
- [ ] Integration test: in-process server, 2 clients, publish + receive verified
- [ ] `benches/` with criterion: `publish_1_sub` and `publish_N_subs` baselines committed
- [ ] Flame graph generated under load; hot paths identified and noted
- [ ] Simulator extended: latency reporting + summary on exit
- [ ] CI check (optional): `cargo test && cargo bench --no-run` to catch compile breaks

---

## Goal 4 — ACKs + durable sessions

### Why ACKs before fanout

Multi-broker fanout built on top of fire-and-forget delivery means messages can silently
disappear with no way to detect it. ACKs establish a concrete delivery contract at the
single-broker level first. When fanout is added later, the question "is this message
done?" already has a clear answer: every durable subscriber has ACKed it.

### Protocol changes

Add two new proto messages:

```protobuf
// Client → Server: acknowledge a delivered message
message AckCmd {
  string message_id = 1;
}

// Updated ConnectCmd: client declares session mode
enum SessionMode {
  EPHEMERAL = 0;   // current behavior — state lost on disconnect
  DURABLE   = 1;   // broker remembers subscriptions + last ACK per topic
}

message ConnectCmd {
  string client_name  = 1;
  SessionMode session_mode = 2;     // new
  string session_id   = 3;          // empty on first connect; broker assigns and returns it
}
```

The broker returns the assigned `session_id` in the first `ServerEvent` after a durable
connect. The client persists this ID locally (env var, file, whatever) and sends it back
on reconnect.

### Broker-side state

For durable clients the broker must persist:
- `session_id → { subscribed_topics, last_acked_id_per_topic }`

This is the first time the broker needs durable storage. A simple embedded key-value
store (e.g. `sled`, or even a JSON file for starters) is enough before the full WAL
(Goal 6). The point here is the protocol and delivery semantics, not storage
sophistication.

### Delivery semantics

| Mode | Guarantee | How |
|---|---|---|
| Ephemeral | At-most-once | Fire and forget; no ACK |
| Durable | At-least-once | Broker retains message until client ACKs; retransmits on reconnect |
| Exactly-once (stretch) | Exactly-once | Client deduplicates by message ID |

At-least-once is the target. Exactly-once is a stretch goal — it requires the client to
track seen message IDs and skip duplicates, which is straightforward but adds client
complexity.

### Retention

The broker cannot retain messages forever for disconnected durable clients. A configurable
**retention window** (e.g. `DURABLE_RETENTION_HOURS=24` in `.env`) determines how long
unACKed messages are kept per session. Clients that reconnect after the window miss those
messages and resume from the oldest available.

### Milestone checklist

- [ ] Add `AckCmd` to proto; plumb through `ClientEvent` oneof
- [ ] Add `session_mode` + `session_id` to `ConnectCmd`; broker returns assigned ID
- [ ] Broker stores durable session state (subscriptions + ACK cursor) in embedded KV
- [ ] On durable reconnect: broker re-subscribes client and replays unACKed messages
- [ ] Retention policy: configurable TTL; expired messages pruned from session state
- [ ] Simulator extended: durable clients that disconnect, reconnect, and verify replay

---

## Goal 5 — Cross-broker fanout & load balancer

### The core problem

Putting two brokers behind a load balancer without any coordination breaks delivery
correctness: a message published on broker A will never reach a subscriber connected to
broker B. **A load balancer is useless until cross-broker fanout is solved.**

### Approach: shared backplane

Each broker connects to a shared backplane (e.g. Redis Pub/Sub, NATS, or a hand-rolled
TCP mesh). On publish, the broker:
1. Delivers locally to any connected subscribers.
2. Forwards the message onto the backplane.

Every broker receives every backplane message and delivers it to its own local subscribers.

### What this teaches

- Why stateful services and load balancers interact differently than stateless HTTP handlers
- Exactly-once vs. at-least-once delivery semantics
- The cost of cross-node fanout vs. local delivery
- How shard-by-topic routing (send topic X always to broker N) eliminates the fanout
  problem at the cost of uneven load distribution

### Milestone checklist

- [ ] Two broker instances sharing a Redis/NATS backplane
- [ ] Load balancer (nginx stream / HAProxy / hand-rolled) in front
- [ ] Simulator verified: messages published on one broker arrive at subscribers on the other
- [ ] Benchmark: measure per-message latency added by the backplane hop

---

## Goal 6 — Performance: zero-copy, batching, lock-free

**Benchmarks are already in place from Goal 3.** Before touching anything, run the
harness and record the baseline. All changes in this goal must show a measurable delta.
a criterion benchmark or a custom tokio harness. Without numbers, you cannot know whether
a change helped.

### Lock-free subscriber map

Replace `RwLock<HashMap>` with a sharded or lock-free concurrent map. Options:
- `dashmap` — sharded `RwLock`, low effort, meaningful improvement
- Hand-rolled sharded map — shard by `topic_name.hash() % N_SHARDS`, each shard has its
  own `Mutex`; teaches sharding as a contention-reduction technique
- `crossbeam`-based epoch-reclaimed map — lock-free reads, teaches epoch-based memory
  reclamation (a key primitive in concurrent data structures)

> **Note on wait-free vs. lock-free:** True wait-free structures (every thread makes
> progress in bounded steps regardless of others) are research-level hard to implement
> correctly, especially across async tasks. Lock-free (CAS-based, no thread ever blocks
> another, but may retry) is realistic and teaches the same fundamental ideas. Aim for
> lock-free; treat wait-free as a stretch goal.

### Zero-copy message delivery

Today each `ServerEvent` is cloned per subscriber on the publish path. Replace with
`Arc<ServerEvent>` so all subscriber delivery shares one allocation. True kernel-level
zero-copy (e.g. `sendfile`, `io_uring`) is only relevant if the broker writes to disk or
sockets directly — that becomes relevant in Goal 5.

### Cache-friendly layout

The subscriber map is a `HashMap<String, HashMap<String, Sender>>` — pointer-chasing on
the hot path. Alternatives to explore:
- Topic IDs as `u32` keys (avoids string hashing on every publish)
- `Vec`-backed subscriber list per topic with sorted inserts (sequential scan for small
  subscriber counts is faster than hashing due to cache locality)
- False-sharing avoidance: pad hot atomic counters to 64 bytes (one cache line) so
  concurrent increments on different cores do not bounce the same cache line

### Batching

Accumulate multiple publish requests and flush them in one pass over the subscriber map
(one read-lock acquisition, N message deliveries). Reduces lock overhead proportional to
batch size. Teaches the latency vs. throughput tradeoff: batching improves throughput but
adds latency for the first message in a batch.

### Milestone checklist

- [ ] Benchmark harness in place (messages/sec, p50/p99 latency)
- [ ] Subscriber map replaced with sharded/lock-free structure; benchmark delta recorded
- [ ] `Arc<ServerEvent>` on publish path; allocations/op confirmed reduced
- [ ] Cache-line padding on `AtomicU32` counters
- [ ] Batch-publish path implemented and benchmarked

---

## Goal 7 — Durable replicated log (WAL + replication + fault tolerance)

Persistence and fault tolerance are the same goal: a broker that crashes and restarts
must not lose messages and must resume serving clients quickly. The mechanism for both is
a **replicated, durable log**. By this point durable session replay (Goal 4) already
exists using an embedded KV store; Goal 7 replaces that with a proper WAL and adds
replication so the log survives broker failures.

### Message IDs

Use **UUIDv7** or **ULID** instead of a random UUID or a plain `AtomicU32`. These are
time-ordered, which means:
- Monotonically increasing → efficient B-tree / LSM index appends (no random I/O)
- Contains a timestamp → enables time-based compaction and replay-from-timestamp
- Random suffix → avoids hot-spot writes to a single B-tree node under concurrent inserts

> UUIDv4 (random) is bad for storage indexes: random inserts scatter across the B-tree,
> causing cache misses and write amplification. Always use a time-ordered ID when the
> value will be stored in a database.

### Write-ahead log (WAL)

Before delivering any message, append it to a WAL on disk. On restart, replay the WAL
to rebuild in-memory state. This is how Postgres, SQLite, and Kafka durability work.

Simple implementation: append-only file of length-prefixed protobuf records. Teaches
`fsync` semantics, durability vs. persistence, and the cost of synchronous disk writes.

The WAL is also the source of truth for session replay: when a durable client reconnects,
the broker seeks to the client's last ACKed message ID in the WAL and streams forward.

### Replication

Run a primary broker and one or more replicas. Primary appends to its WAL and replicates
to replicas before acknowledging to the publisher. Options:
- **Primary/replica with synchronous replication** — simplest; primary blocks until at
  least one replica confirms the write
- **Raft consensus** — correct under arbitrary failure; significantly more complex to
  implement but teaches leader election, log matching, and split-brain prevention

Start with primary/replica. Raft is a stretch goal.

### Fault tolerance patterns

- **Durable session reconnect:** client sends its `session_id` and last ACKed message ID;
  broker replays unACKed messages from the WAL
- **Backup routing:** if a broker replica detects the primary is down (missed heartbeats),
  it promotes itself and redirects clients; requires a coordination service (etcd, ZooKeeper,
  or a hand-rolled heartbeat protocol)

### Milestone checklist

- [ ] Switch message IDs to UUIDv7 / ULID
- [ ] Append-only WAL: every publish written to disk before delivery (`fsync` path)
- [ ] WAL replay on restart: broker recovers in-memory state from log
- [ ] WAL compaction: configurable retention, expired segments pruned
- [ ] Session replay upgraded to use WAL (replaces embedded KV from Goal 4)
- [ ] Primary/replica replication: primary waits for at-least-one replica WAL ACK before confirming publish
- [ ] Replica promotion on primary failure (hand-rolled heartbeat + takeover)

---

## Code review fixes (2026-03-06)

Working through 15 bugs identified in `Code Review/2026-03-06.md`. Fix order follows the
review's recommended priority (correctness first, then performance, then style).

### #9 — `Box<dyn Error + Send + Sync>` across all client methods (`src/client/mod.rs`) ✅

Every public method on `Client` previously returned `Result<_, Box<dyn std::error::Error>>`.
`Box<dyn Error>` carries no thread-safety guarantees, so holding one across an `.await`
inside `tokio::spawn` fails to compile — `spawn` requires the future to be `Send`.

The fix adds `+ Send + Sync` to every return type:

```rust
// before
Result<Self, Box<dyn std::error::Error>>

// after
Result<Self, Box<dyn std::error::Error + Send + Sync>>
```

`Send` means the value is safe to move to another thread. `Sync` means it's safe to
reference from multiple threads. Together they satisfy `tokio::spawn`'s requirement.
This also eliminates the simulator workaround where `Client` had to be extracted from
the `Result` before the first `.await`.

---

## Running locally

```bash
# terminal 1
cargo run --bin server

# terminal 2
cargo run --bin simulator -- 3 200

# or run a single interactive client
cargo run --bin client
```

Configuration is in `.env` (not committed):

```
SERVER_HOST=::1
SERVER_PORT=50051
NUM_CLIENTS=3
SPAWN_INTERVAL_MS=500
```
