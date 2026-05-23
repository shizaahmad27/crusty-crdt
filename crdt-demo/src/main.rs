//! Peer-to-peer demo for `crdt-lib`.
//!
//! Hver node holder en delt `OrSet<String>` (en handleliste) og synkroniserer
//! periodisk med sine peers via en enkel gossip-protokoll over TCP.
//!
//! ## Bruk
//!
//! Start tre noder i hver sin terminal:
//!
//! ```bash
//! cargo run --bin crdt-demo -- --id 1 --port 7001 --peers 127.0.0.1:7002,127.0.0.1:7003
//! cargo run --bin crdt-demo -- --id 2 --port 7002 --peers 127.0.0.1:7001,127.0.0.1:7003
//! cargo run --bin crdt-demo -- --id 3 --port 7003 --peers 127.0.0.1:7001,127.0.0.1:7002
//! ```
//!
//! Hver node viser en prompt der du kan skrive kommandoer:
//!
//! - `add <element>` — legg til et element
//! - `remove <element>` — fjern et element
//! - `list` — vis nåværende innhold
//! - `quit` — avslutt
//!
//! Endringer propagerer automatisk til peers via gossip.

mod node;
mod protocol;

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;

use crate::node::spawn_node;

/// Kommandolinje-argumenter for én node.
#[derive(Parser, Debug)]
#[command(name = "crdt-demo", about = "Peer-to-peer CRDT demo")]
struct Args {
    /// Replika-ID for denne noden. Må være unik på tvers av alle noder.
    #[arg(long)]
    id: u64,

    /// Lytte-port for denne noden.
    #[arg(long)]
    port: u16,

    /// Komma-separert liste over peer-adresser, f.eks. `127.0.0.1:7002,127.0.0.1:7003`.
    #[arg(long, value_delimiter = ',')]
    peers: Vec<SocketAddr>,

    /// Hvor ofte vi sender gossip, i millisekunder.
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
        .context("ugyldig lytte-adresse")?;

    let state = spawn_node(
        args.id,
        listen_addr,
        args.peers,
        Duration::from_millis(args.gossip_ms),
    )
    .await?;

    println!(
        "Node {} kjører. Kommandoer: add <x>, remove <x>, list, quit",
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
                println!("la til '{arg}'");
            }
            "remove" | "rm" if !arg.is_empty() => {
                state.lock().await.remove(arg);
                println!("fjernet '{arg}'");
            }
            "list" | "ls" => {
                let mut items: Vec<String> = state.lock().await.list.iter().cloned().collect();
                items.sort();
                if items.is_empty() {
                    println!("(tom)");
                } else {
                    for item in items {
                        println!("  - {item}");
                    }
                }
            }
            "quit" | "exit" => break,
            _ => println!("ukjent kommando. prøv: add <x>, remove <x>, list, quit"),
        }
    }

    Ok(())
}
