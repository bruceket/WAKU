//! Servizio comandi transazionale a singolo writer (G0).

pub mod command;
pub mod journal;
pub mod service;
pub mod summary;

pub use command::*;
pub use journal::*;
pub use service::*;
pub use summary::model_summary;
