//! Wire protocol for peer-to-peer nodes.
//!
//! Each message is sent as `[u32 length in big-endian][bincode payload]`. This
//! lets the receiver read one complete message at a time without guessing where
//! it ends. `bincode` is used because it is compact and works out of the box
//! with `serde`-derived types.

use std::io;

use crdt_lib::set::OrSet;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Message types exchanged between nodes.
///
/// The demo sends the full state on every gossip round. This is idempotent
/// (merge handles seeing the same state multiple times) but not
/// bandwidth-efficient. A production version would use delta-state CRDTs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Full state of the shared shopping list.
    State(OrSet<String>),
}

/// Writes one message to a stream with a 4-byte length prefix.
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

/// Reads one message from a stream. Returns `Ok(None)` on EOF.
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
