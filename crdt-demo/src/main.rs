//! Peer-to-peer demo for `crdt-lib`.
//!
//! Each node holds a shared `OrSet<String>` (a shopping list) and periodically
//! synchronizes with its peers via a simple gossip protocol over TCP.
//! See README.md file for usage instructions


mod node;
mod protocol;

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;

use crate::node::spawn_node;

/// Command-line arguments for a single node.
#[derive(Parser, Debug)]
#[command(name = "crdt-demo", about = "Peer-to-peer CRDT demo")]
struct Args {
    /// Replica ID for this node. Must be unique across all nodes.
    #[arg(long)]
    id: u64,

    /// Listening port for this node.
    #[arg(long)]
    port: u16,

    /// Comma-separated list of peer addresses, e.g. `127.0.0.1:7002,127.0.0.1:7003`.
    #[arg(long, value_delimiter = ',')]
    peers: Vec<SocketAddr>,

    /// How often to send gossip, in milliseconds.
    #[arg(long, default_value_t = 2000)]
    gossip_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let listen_addr: SocketAddr = format!("0.0.0.0:{}", args.port)
        .parse()
        .context("invalid listen address")?;

    let state = spawn_node(
        args.id,
        listen_addr,
        args.peers,
        Duration::from_millis(args.gossip_ms),
    )
    .await?;

    println!(
        "Node {} running. Commands: add <x>, remove <x>, list, quit",
        args.id
    );

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match cmd {
            "add" if !arg.is_empty() => {
                state.lock().await.add(arg.to_string());
                println!("added '{arg}'");
            }
            "remove" | "rm" if !arg.is_empty() => {
                state.lock().await.remove(arg);
                println!("removed '{arg}'");
            }
            "list" | "ls" => {
                let mut items: Vec<String> = state.lock().await.list.iter().cloned().collect();
                items.sort();
                if items.is_empty() {
                    println!("(empty)");
                } else {
                    for item in items {
                        println!("  - {item}");
                    }
                }
            }
            "quit" | "exit" => break,
            _ => println!("unknown command. try: add <x>, remove <x>, list, quit"),
        }
    }

    Ok(())
}
