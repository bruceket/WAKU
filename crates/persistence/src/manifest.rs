//! Manifest dell'archivio `.waku`.

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "waku";
pub const VERSION: u32 = 1;
pub const MANIFEST_NAME: &str = "manifest.json";
pub const MODEL_NAME: &str = "model.json";

/// Contenuto di `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub app: String,
    /// Data di creazione, RFC3339 (UTC).
    pub created: String,
    pub model: String,
}

impl Manifest {
    pub fn new() -> Manifest {
        Manifest {
            format: FORMAT.to_owned(),
            version: VERSION,
            app: env!("CARGO_PKG_VERSION").to_owned(),
            created: humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string(),
            model: MODEL_NAME.to_owned(),
        }
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}
