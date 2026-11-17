//! Client del bridge IPC (lato `waku-mcp` e test): una connessione, `Hello`
//! con token letto da file, poi richieste/risposte a frame.

use std::path::Path;

use thiserror::Error;

use crate::protocol::{BRIDGE_PROTOCOL, BridgeError, BridgeRequest, BridgeResponse};

/// Messaggio mostrato quando il desktop non e' in esecuzione.
pub const NO_SESSION_MESSAGE: &str =
    "Nessuna sessione Waku aperta. Avvia Waku e concedi l'accesso AI.";

#[derive(Debug, Error)]
pub enum ClientError {
    /// Pipe assente o file token mancante/illeggibile.
    #[error("no_session: {NO_SESSION_MESSAGE}")]
    NoSession,
    /// Errore applicativo restituito dal bridge.
    #[error("{code}: {message}")]
    Bridge { code: String, message: String },
    /// Errore di trasporto o di formato.
    #[error("bridge: {0}")]
    Transport(#[from] BridgeError),
    /// Risposta inattesa o connessione chiusa.
    #[error("protocol: {0}")]
    Protocol(String),
}

impl ClientError {
    /// Codice breve (`no_session`, codice bridge, `transport`, `protocol`).
    pub fn code(&self) -> &str {
        match self {
            ClientError::NoSession => "no_session",
            ClientError::Bridge { code, .. } => code,
            ClientError::Transport(_) => "transport",
            ClientError::Protocol(_) => "protocol",
        }
    }
}

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

pub struct BridgeClient {
    #[cfg(windows)]
    stream: Stream,
    #[cfg(not(windows))]
    _never: std::convert::Infallible,
}

impl BridgeClient {
    /// Apre la pipe, legge il token da `token_dir` ed esegue l'handshake.
    #[cfg(windows)]
    pub async fn connect(pipe: &str, token_dir: &Path) -> Result<Self, ClientError> {
        use tokio::net::windows::named_pipe::ClientOptions;
        const ERROR_PIPE_BUSY: i32 = 231;

        let token = crate::token::read_token_file(token_dir).map_err(|_| ClientError::NoSession)?;
        let mut tries = 0;
        let stream = loop {
            match ClientOptions::new().open(pipe) {
                Ok(s) => break s,
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < 50 => {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(ClientError::NoSession);
                }
                Err(e) => return Err(ClientError::Transport(e.into())),
            }
        };
        let mut client = BridgeClient { stream };
        match client
            .request(BridgeRequest::Hello {
                token: token.as_str().to_string(),
                protocol: BRIDGE_PROTOCOL,
            })
            .await?
        {
            BridgeResponse::Ok { .. } => Ok(client),
            other => Err(ClientError::Protocol(format!(
                "handshake inatteso: {other:?}"
            ))),
        }
    }

    /// Stub non Windows: il bridge in G0 esiste solo su Windows (R5).
    #[cfg(not(windows))]
    pub async fn connect(_pipe: &str, _token_dir: &Path) -> Result<Self, ClientError> {
        let _ = BRIDGE_PROTOCOL;
        Err(ClientError::NoSession)
    }

    /// Invia una richiesta e legge una risposta. `Err` del bridge diventa
    /// `ClientError::Bridge`.
    #[cfg(windows)]
    pub async fn request(&mut self, req: BridgeRequest) -> Result<BridgeResponse, ClientError> {
        use crate::protocol::{read_frame, write_frame};
        let body = serde_json::to_vec(&req).map_err(BridgeError::from)?;
        write_frame(&mut self.stream, &body).await?;
        let resp = read_frame(&mut self.stream)
            .await?
            .ok_or_else(|| ClientError::Protocol("connessione chiusa dal bridge".into()))?;
        match serde_json::from_slice(&resp).map_err(BridgeError::from)? {
            BridgeResponse::Err { code, message } => Err(ClientError::Bridge { code, message }),
            ok => Ok(ok),
        }
    }

    #[cfg(not(windows))]
    pub async fn request(&mut self, _req: BridgeRequest) -> Result<BridgeResponse, ClientError> {
        match self._never {}
    }
}
