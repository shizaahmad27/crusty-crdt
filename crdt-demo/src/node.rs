//! Node state, gossip loop, and incoming peer connections.
//!
//! State is shared via `Arc<Mutex<...>>`. Two background tasks run per node:
//! one listens for incoming connections, one sends periodic gossip to all peers.
//! A new TCP connection is opened for each gossip round to keep the code simple.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crdt_lib::clock::LamportClock;
use crdt_lib::set::OrSet;
use crdt_lib::{Crdt, ReplicaId};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::protocol::{read_message, write_message, Message};

/// Shared state for a node. Wrapped in `Arc<Mutex<...>>` by callers so
/// multiple tokio tasks can access it.
pub struct NodeState {
    /// The replica ID of this node, used as the Lamport clock owner.
    #[allow(dead_code)]
    pub replica: ReplicaId,
    /// Local Lamport clock, used to generate unique tags for add operations.
    pub clock: LamportClock,
    /// The shared shopping list.
    pub list: OrSet<String>,
}

impl NodeState {
    /// Creates a new node state.
    pub fn new(replica: ReplicaId) -> Self {
        Self {
            replica,
            clock: LamportClock::new(replica),
            list: OrSet::new(),
        }
    }

    /// Adds an item to the list and advances the Lamport clock.
    pub fn add(&mut self, item: String) {
        let tag = self.clock.tick();
        self.list.add(item, tag);
    }

    /// Removes an item from the list.
    pub fn remove(&mut self, item: &str) {
        self.list.remove(&item.to_string());
    }

    /// Merges another state into this one and advances the clock past all
    /// observed tags so future local adds get strictly higher timestamps.
    pub fn merge(&mut self, other: &OrSet<String>) {
        self.list.merge(other);
        let _ = self.clock.observe(self.clock.current());
    }
}

/// Starts a node: binds the listen port, launches the gossip loop, and returns
/// the shared state so the main task can run the REPL against it.
pub async fn spawn_node(
    replica: ReplicaId,
    listen_addr: SocketAddr,
    peers: Vec<SocketAddr>,
    gossip_interval: Duration,
) -> Result<Arc<Mutex<NodeState>>> {
    let state = Arc::new(Mutex::new(NodeState::new(replica)));

    // Task 1: accept incoming connections and merge their state.
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "listening for incoming peers");
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        debug!(%peer_addr, "incoming connection");
                        let state = Arc::clone(&state);
                        tokio::spawn(handle_incoming(stream, state));
                    }
                    Err(e) => warn!(error = %e, "accept failed"),
                }
            }
        });
    }

    // Task 2: periodically send gossip to all peers.
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let mut ticker = interval(gossip_interval);
            ticker.tick().await; // first tick fires immediately; skip it
            loop {
                ticker.tick().await;
                let snapshot = {
                    let s = state.lock().await;
                    s.list.clone()
                };
                for peer in &peers {
                    if let Err(e) = gossip_to_peer(*peer, &snapshot).await {
                        debug!(%peer, error = %e, "gossip failed");
                    }
                }
            }
        });
    }

    Ok(state)
}

/// Handles an incoming connection: reads messages and merges them in.
async fn handle_incoming(mut stream: TcpStream, state: Arc<Mutex<NodeState>>) {
    loop {
        match read_message(&mut stream).await {
            Ok(Some(Message::State(other))) => {
                let mut s = state.lock().await;
                s.merge(&other);
                debug!(size = s.list.len(), "merged peer state");
            }
            Ok(None) => break,
            Err(e) => {
                debug!(error = %e, "read failed");
                break;
            }
        }
    }
}

/// Sends one gossip message to a peer: opens a connection, writes, and closes.
async fn gossip_to_peer(peer: SocketAddr, list: &OrSet<String>) -> Result<()> {
    let mut stream = TcpStream::connect(peer).await?;
    write_message(&mut stream, &Message::State(list.clone())).await?;
    Ok(())
}
