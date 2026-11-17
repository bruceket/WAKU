//! Protocollo del bridge: frame `u32` LE di lunghezza + JSON (max 1 MiB).

use commands::Action;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Versione corrente del protocollo bridge.
pub const BRIDGE_PROTOCOL: u32 = 1;
/// Dimensione massima del corpo di un frame.
pub const MAX_FRAME: usize = 1024 * 1024;

pub fn supported_protocols() -> &'static [u32] {
    &[BRIDGE_PROTOCOL]
}

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("frame di {0} byte oltre il limite di {MAX_FRAME}")]
    TooLarge(usize),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("bridge non supportato su questa piattaforma (solo Windows in G0)")]
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BridgeRequest {
    Hello {
        token: String,
        protocol: u32,
    },
    Summary,
    Envelope {
        request_id: String,
        expected_revision: u64,
        actions: Vec<Action>,
        preview: bool,
    },
    Undo {
        expected_revision: u64,
    },
    Redo {
        expected_revision: u64,
    },
}

/// Risposta del bridge (`{"type": ..., "body": ...}`).
///
/// Convenzione handshake: un `Hello` valido riceve
/// `Ok { revision: <corrente>, created: [], preview: false }`.
/// Codici di errore: `forbidden`, `conflict`, `invalid`, `bad_token`,
/// `too_large`, `unsupported_protocol`, `busy`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum BridgeResponse {
    Ok {
        revision: u64,
        created: Vec<u64>,
        preview: bool,
    },
    Summary(Value),
    Err {
        code: String,
        message: String,
    },
}

impl BridgeResponse {
    pub fn err(code: &str, message: impl Into<String>) -> Self {
        BridgeResponse::Err {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// Legge un frame. `Ok(None)` a fine stream pulita. Un header oltre il limite
/// produce `TooLarge` senza leggere il corpo.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Vec<u8>>, BridgeError> {
    let mut hdr = [0u8; 4];
    match r.read_exact(&mut hdr).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(hdr) as usize;
    if len > MAX_FRAME {
        return Err(BridgeError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    Ok(Some(body))
}

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, body: &[u8]) -> Result<(), BridgeError> {
    if body.len() > MAX_FRAME {
        return Err(BridgeError::TooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_le_bytes()).await?;
    w.write_all(body).await?;
    w.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_roundtrip_and_limit() {
        let (mut a, mut b) = tokio::io::duplex(64);
        tokio::spawn(async move { write_frame(&mut a, b"{}").await.unwrap() });
        assert_eq!(read_frame(&mut b).await.unwrap().unwrap(), b"{}");
        assert!(read_frame(&mut b).await.unwrap().is_none());

        let hdr = ((MAX_FRAME + 1) as u32).to_le_bytes();
        let mut r: &[u8] = &hdr;
        assert!(matches!(
            read_frame(&mut r).await,
            Err(BridgeError::TooLarge(_))
        ));
    }
}
