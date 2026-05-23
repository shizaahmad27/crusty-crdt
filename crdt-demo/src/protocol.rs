//! Wire-protokoll for peer-to-peer-noder.
//!
//! Hver melding sendes som `[u32 lengde i big-endian][bincode-payload]`. Dette
//! gjør at mottakeren kan lese én melding av gangen uten å gjette hvor den
//! slutter. `bincode` er valgt fordi det er kompakt og fungerer rett ut av
//! boksen med `serde`-deriverte typer.

use std::io;

use crdt_lib::set::OrSet;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Meldingstyper som utveksles mellom noder.
///
/// I dette demoet sender vi hele tilstanden ved hver gossip-runde. Dette er
/// idempotent (merge tåler å se samme tilstand mange ganger) men ikke
/// båndbreddeoptimalt. En produksjonsversjon ville brukt delta-state CRDTs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Full tilstand for den delte handlelisten.
    State(OrSet<String>),
}

/// Skriver én melding til en strøm med 4-bytes lengdeprefiks.
pub async fn write_message<W>(stream: &mut W, message: &Message) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let payload =
        bincode::serialize(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "message too large"))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;
    Ok(())
}

/// Leser én melding fra en strøm. Returnerer `Ok(None)` ved EOF.
pub async fn read_message<R>(stream: &mut R) -> io::Result<Option<Message>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;

    let message = bincode::deserialize(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(message))
}
