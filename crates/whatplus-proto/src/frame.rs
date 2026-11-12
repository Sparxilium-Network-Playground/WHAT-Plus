//! 封包：4 bytes big-endian 長度 + JSON 內容。

use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// 單一封包上限。超過就斷線，避免對端讓我們配置大量記憶體。
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("connection error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame too large: {0} bytes")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
}

pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(body.len()));
    }
    writer.write_all(&(body.len() as u32).to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

/// 讀取一個封包。對端在封包邊界正常關閉時回傳 `Ok(None)`。
pub async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>, FrameError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len = [0u8; 4];
    match reader.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;

    #[tokio::test]
    async fn round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &Request::Status).await.unwrap();
        let mut reader = &buf[..];
        let got: Option<Request> = read_frame(&mut reader).await.unwrap();
        assert!(matches!(got, Some(Request::Status)));
        let end: Option<Request> = read_frame(&mut reader).await.unwrap();
        assert!(end.is_none());
    }

    #[tokio::test]
    async fn rejects_oversized_length() {
        let buf = (u32::MAX).to_be_bytes();
        let mut reader = &buf[..];
        let got: Result<Option<Request>, _> = read_frame(&mut reader).await;
        assert!(matches!(got, Err(FrameError::TooLarge(_))));
    }
}
