//! Policy di scrittura AI: concessa solo dall'utente via GUI.

use std::sync::atomic::{AtomicBool, Ordering};

/// Stato del permesso di scrittura AI. Default: negato (AC-18).
#[derive(Debug, Default)]
pub struct Policy {
    write_granted: AtomicBool,
}

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }
    /// Concede la scrittura (azione esplicita dell'utente).
    pub fn grant(&self) {
        self.write_granted.store(true, Ordering::SeqCst);
    }
    /// Revoca la scrittura.
    pub fn revoke(&self) {
        self.write_granted.store(false, Ordering::SeqCst);
    }
    pub fn allows_write(&self) -> bool {
        self.write_granted.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_denied_grant_revoke() {
        let p = Policy::new();
        assert!(!p.allows_write());
        p.grant();
        assert!(p.allows_write());
        p.revoke();
        assert!(!p.allows_write());
    }
}
