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

/// Scrive un file arbitrario in modo atomico (tmp + `sync_all` + `rename`).
/// `f` riceve uno scrittore bufferizzato; il buffer è svuotato esplicitamente
/// prima della sincronizzazione. Su errore (di `f` o del flush) il file di
/// destinazione preesistente resta intatto e il tmp è rimosso.
pub fn write_atomic_with<F>(path: &Path, f: F) -> Result<(), PersistError>
where
    F: FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
{
    write_atomic(path, |tmp| {
        let mut w = std::io::BufWriter::new(std::fs::File::create(tmp)?);
        f(&mut w)?;
        w.into_inner().map_err(|e| e.into_error())?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("waku-atomic-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_atomic_with_failure_keeps_previous_file() {
        let d = dir("fail");
        let p = d.join("out.ifc");
        std::fs::write(&p, b"VALIDO-PRECEDENTE").unwrap();
        let r = write_atomic_with(&p, |w| {
            w.write_all(b"ISO-10303-21;\nmeta")?;
            Err(std::io::Error::other("errore a meta' export"))
        });
        assert!(r.is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"VALIDO-PRECEDENTE");
        assert!(!tmp_path(&p).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_atomic_with_success_flushes_all() {
        let d = dir("ok");
        let p = d.join("out.ifc");
        std::fs::write(&p, b"vecchio").unwrap();
        let big = vec![b'x'; 100_000];
        write_atomic_with(&p, |w| w.write_all(&big)).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), big);
        assert!(!tmp_path(&p).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
