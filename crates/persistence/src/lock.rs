//! Lock di progetto: file `<path>.lock` creato in modo esclusivo.
//!
//! Limite G0: un lock rimasto dopo un crash (stale) NON viene rilevato né
//! recuperato; va rimosso a mano. La gestione dei lock obsoleti è rimandata.

use crate::PersistError;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Possesso esclusivo del progetto; il file di lock è rimosso al Drop.
#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
}

fn lock_path(path: &Path) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}

impl ProjectLock {
    /// Acquisisce il lock; `Locked` se esiste già.
    pub fn acquire(path: &Path) -> Result<ProjectLock, PersistError> {
        let lp = lock_path(path);
        let mut f = match OpenOptions::new().write(true).create_new(true).open(&lp) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(PersistError::Locked),
            Err(e) => return Err(e.into()),
        };
        let content = format!("{}@{}", std::process::id(), hostname());
        if let Err(e) = f.write_all(content.as_bytes()) {
            drop(f);
            let _ = std::fs::remove_file(&lp);
            return Err(e.into());
        }
        Ok(ProjectLock { path: lp })
    }

    /// Percorso del file di lock.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
