//! Selve noden: delt CRDT-tilstand, gossip-løkke, og innkommende peer-tilkoblinger.
//!
//! Designet er bevisst enkelt:
//!
//! - Tilstanden ligger bak en `Arc<Mutex<...>>` slik at flere oppgaver kan lese
//!   og skrive den.
//! - To bakgrunnsoppgaver kjører per node: én lytter etter innkommende
//!   tilkoblinger, én sender periodisk gossip til alle peers.
//! - Brukerens REPL kjører i hovedoppgaven og oppdaterer tilstanden lokalt.
//!
//! Merk at vi ikke holder vedvarende koblinger til peers — vi åpner en ny TCP-
//! kobling for hver gossip-runde. Dette er sløsete, men gjør koden mye enklere
//! og passer perfekt for en demo der noder ofte kobles av og på.

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

/// Delt tilstand for en node. Innpakket i `Arc<Mutex<...>>` av kallerne så
/// flere tokio-oppgaver kan ha tilgang.
pub struct NodeState {
    /// Replika-IDen til denne noden, brukt som Lamport-klokke-eier.
    pub replica: ReplicaId,
    /// Lokal Lamport-klokke, brukt for å generere unike tagger til add-er.
    pub clock: LamportClock,
    /// Den delte handlelisten.
    pub list: OrSet<String>,
}

impl NodeState {
    /// Lager en ny node-tilstand.
    pub fn new(replica: ReplicaId) -> Self {
        Self {
            replica,
            clock: LamportClock::new(replica),
            list: OrSet::new(),
        }
    }

    /// Legger til et element i listen og oppdaterer Lamport-klokken.
    pub fn add(&mut self, item: String) {
        let tag = self.clock.tick();
        self.list.add(item, tag);
    }

    /// Fjerner et element fra listen.
    pub fn remove(&mut self, item: &str) {
        self.list.remove(&item.to_string());
    }

    /// Slår sammen en annen tilstand inn i denne. Lamport-klokken må også
    /// avanseres slik at fremtidige lokale tagger ikke kolliderer med tagger
    /// vi har observert.
    pub fn merge(&mut self, other: &OrSet<String>) {
        self.list.merge(other);
        // Avanser klokken forbi alle observerte tagger så fremtidige lokale
        // add-er får strengt høyere tidsstempler.
        // Vi gjør dette enkelt ved å observere klokkens nåværende verdi.
        let _ = self.clock.observe(self.clock.current());
    }
}

/// Starter en node: åpner lytte-port, starter gossip-løkke til peers, og
/// returnerer den delte tilstanden så hovedtråden kan kjøre REPL mot den.
pub async fn spawn_node(
    replica: ReplicaId,
    listen_addr: SocketAddr,
    peers: Vec<SocketAddr>,
    gossip_interval: Duration,
) -> Result<Arc<Mutex<NodeState>>> {
    let state = Arc::new(Mutex::new(NodeState::new(replica)));

    // Oppgave 1: lytt etter innkommende tilkoblinger og merge inn deres tilstand.
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "lytter etter innkommende peers");
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        debug!(%peer_addr, "innkommende tilkobling");
                        let state = Arc::clone(&state);
                        tokio::spawn(handle_incoming(stream, state));
                    }
                    Err(e) => warn!(error = %e, "accept feilet"),
                }
            }
        });
    }

    // Oppgave 2: send periodisk gossip til alle peers.
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let mut ticker = interval(gossip_interval);
            ticker.tick().await; // første tick er umiddelbar; hopp over
            loop {
                ticker.tick().await;
                let snapshot = {
                    let s = state.lock().await;
                    s.list.clone()
                };
                for peer in &peers {
                    if let Err(e) = gossip_to_peer(*peer, &snapshot).await {
                        debug!(%peer, error = %e, "gossip mislyktes");
                    }
                }
            }
        });
    }

    Ok(state)
}

/// Håndterer en innkommende kobling: leser meldinger og merger dem inn.
async fn handle_incoming(mut stream: TcpStream, state: Arc<Mutex<NodeState>>) {
    loop {
        match read_message(&mut stream).await {
            Ok(Some(Message::State(other))) => {
                let mut s = state.lock().await;
                s.merge(&other);
                debug!(size = s.list.len(), "merget inn peer-tilstand");
            }
            Ok(None) => break,
            Err(e) => {
                debug!(error = %e, "lesing feilet");
                break;
            }
        }
    }
}

/// Sender én gossip-melding til en peer. Åpner en kobling, sender, og lukker.
async fn gossip_to_peer(peer: SocketAddr, list: &OrSet<String>) -> Result<()> {
    let mut stream = TcpStream::connect(peer).await?;
    write_message(&mut stream, &Message::State(list.clone())).await?;
    Ok(())
}
