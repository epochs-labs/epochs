//! EPX length-prefixed JSON framing.

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};

/// Maximum frame payload (matches epochs-server).
pub const MAX_FRAME: u32 = 16 * 1024 * 1024;

pub async fn write_json<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    let len = u32::try_from(body.len()).map_err(|_| Error::Frame("frame too large".into()))?;
    if len > MAX_FRAME {
        return Err(Error::Frame(format!("frame too large: {len}")));
    }
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_json<R: AsyncRead + Unpin, T: DeserializeOwned>(reader: &mut R) -> Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(Error::Frame(format!("frame too large: {len}")));
    }
    let mut buf = vec![0u8; len as usize];
    if len > 0 {
        reader.read_exact(&mut buf).await?;
    }
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::duplex;

    #[tokio::test]
    async fn roundtrip_json_frame() {
        let (mut a, mut b) = duplex(64 * 1024);
        let payload = json!({"id": 1, "method": "hello", "params": {}});
        write_json(&mut a, &payload).await.unwrap();
        let got: serde_json::Value = read_json(&mut b).await.unwrap();
        assert_eq!(got, payload);
    }
}
