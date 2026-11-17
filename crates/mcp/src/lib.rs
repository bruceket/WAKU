//! Bridge IPC lato desktop (named pipe Windows) verso il server MCP, con
//! policy di scrittura AI e token di sessione.

pub mod bridge_server;
pub mod policy;
pub mod protocol;
pub mod token;

pub use bridge_server::{
    CancellationToken, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, MAX_CONNECTIONS, ServeConfig, pipe_name,
    pipe_name_with_suffix, serve, serve_on, serve_with,
};
pub use policy::Policy;
pub use protocol::{
    BRIDGE_PROTOCOL, BridgeError, BridgeRequest, BridgeResponse, MAX_FRAME, supported_protocols,
};
pub use token::Token;
