use crate::sharing::{Error, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// One logical message inside the encrypted stream. Reject the length before
/// allocating its body; truncation closes the connection, never retries a frame.
pub async fn read(io: &mut (impl AsyncRead + Unpin), limit: usize) -> Result<Vec<u8>> {
    let len = io.read_u32().await? as usize;
    if len == 0 || len > limit {
        return Err(Error::Limit);
    }
    let mut bytes = vec![0; len];
    io.read_exact(&mut bytes).await?;
    Ok(bytes)
}

pub async fn write(io: &mut (impl AsyncWrite + Unpin), bytes: &[u8], limit: usize) -> Result<()> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(Error::Limit);
    }
    let len = u32::try_from(bytes.len()).map_err(|_| Error::Limit)?;
    io.write_u32(len).await?;
    io.write_all(bytes).await?;
    io.flush().await?;
    Ok(())
}

/// The tunnel pump must end with its application task, including on cancellation
/// or revocation with plaintext still buffered in its pipe.
pub(super) struct Tunnel(pub sheprd_tunnel::stream::Tunnel);

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.0.task.abort();
    }
}
