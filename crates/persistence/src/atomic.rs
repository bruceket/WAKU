//! Scrittura atomica: tmp nella stessa directory, `sync_all`, `rename`.

use crate::PersistError;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(s)
}

/// Scrive il contenuto tramite `write` su un file temporaneo, lo sincronizza
/// su disco e lo rinomina sopra `path`. Su errore il tmp è rimosso e il file
/// originale resta intatto. `write` riceve il percorso del tmp e deve crearlo.
pub(crate) fn write_atomic<W>(path: &Path, write: W) -> Result<(), PersistError>
where
    W: FnOnce(&Path) -> Result<(), PersistError>,
{
    let tmp = tmp_path(path);
    let result = (|| {
        write(&tmp)?;
        OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
