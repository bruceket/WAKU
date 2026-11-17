//! Formato nativo `.waku` (archivio zip) con salvataggio atomico e lock di progetto.

mod atomic;
mod lock;
mod manifest;

pub use lock::ProjectLock;
pub use manifest::Manifest;

use domain::Model;
use manifest::{FORMAT, MANIFEST_NAME, MODEL_NAME, VERSION};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Dimensione massima di una entry decompressa (256 MiB).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
/// Numero massimo di entry nell'archivio.
pub const MAX_ENTRIES: usize = 64;

/// Errori di persistenza.
#[derive(Debug, Error)]
pub enum PersistError {
    #[error("errore di I/O: {0}")]
    Io(#[from] io::Error),
    #[error("file corrotto: {0}")]
    Corrupt(String),
    #[error("versione del formato {found} non supportata (massima: {supported})")]
    FutureVersion { found: u32, supported: u32 },
    #[error("progetto già in uso (lock presente)")]
    Locked,
}

impl From<zip::result::ZipError> for PersistError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(e) => PersistError::Io(e),
            other => PersistError::Corrupt(other.to_string()),
        }
    }
}

/// Limiti di lettura (iniettabili per i test).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub max_entry_bytes: u64,
    pub max_entries: usize,
}

const DEFAULT_LIMITS: Limits = Limits {
    max_entry_bytes: MAX_ENTRY_BYTES,
    max_entries: MAX_ENTRIES,
};

/// Salva il modello in modo atomico: l'originale non è mai visibile a metà.
pub fn save(path: &Path, model: &Model) -> Result<(), PersistError> {
    save_with(path, model, write_archive)
}

/// Come [`save`], con scrittore del tmp iniettabile (fault injection nei test).
pub(crate) fn save_with<W>(path: &Path, model: &Model, write: W) -> Result<(), PersistError>
where
    W: FnOnce(&Path, &Model) -> Result<(), PersistError>,
{
    atomic::write_atomic(path, |tmp| write(tmp, model))
}

fn write_archive(tmp: &Path, model: &Model) -> Result<(), PersistError> {
    let file = File::create(tmp)?;
    let mut zw = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let manifest = serde_json::to_vec_pretty(&Manifest::new())
        .map_err(|e| PersistError::Corrupt(e.to_string()))?;
    zw.start_file(MANIFEST_NAME, opts)?;
    zw.write_all(&manifest)?;
    let body = serde_json::to_vec(model).map_err(|e| PersistError::Corrupt(e.to_string()))?;
    zw.start_file(MODEL_NAME, opts)?;
    zw.write_all(&body)?;
    zw.finish()?.sync_all()?;
    Ok(())
}

/// Carica un modello. Una versione futura è un errore e non scrive nulla.
pub fn load(path: &Path) -> Result<Model, PersistError> {
    load_with_limits(path, DEFAULT_LIMITS)
}

pub(crate) fn load_with_limits(path: &Path, limits: Limits) -> Result<Model, PersistError> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    if zip.len() > limits.max_entries {
        return Err(PersistError::Corrupt(format!(
            "troppe entry nell'archivio: {}",
            zip.len()
        )));
    }
    let manifest_bytes = read_entry(&mut zip, MANIFEST_NAME, limits.max_entry_bytes)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| PersistError::Corrupt(format!("manifest: {e}")))?;
    if manifest.format != FORMAT {
        return Err(PersistError::Corrupt(format!(
            "formato sconosciuto: {}",
            manifest.format
        )));
    }
    if manifest.version > VERSION {
        return Err(PersistError::FutureVersion {
            found: manifest.version,
            supported: VERSION,
        });
    }
    if manifest.model != MODEL_NAME {
        return Err(PersistError::Corrupt(format!(
            "entry del modello inattesa: {}",
            manifest.model
        )));
    }
    let body = read_entry(&mut zip, MODEL_NAME, limits.max_entry_bytes)?;
    let mut model: Model =
        serde_json::from_slice(&body).map_err(|e| PersistError::Corrupt(format!("model: {e}")))?;
    model.normalize_ids();
    model
        .validate()
        .map_err(|e| PersistError::Corrupt(e.to_string()))?;
    Ok(model)
}

/// Legge una entry rispettando il limite sia dichiarato sia effettivo
/// (un header che mente non aggira il tetto).
fn read_entry(zip: &mut ZipArchive<File>, name: &str, max: u64) -> Result<Vec<u8>, PersistError> {
    let entry = zip
        .by_name(name)
        .map_err(|e| PersistError::Corrupt(format!("{name}: {e}")))?;
    if entry.size() > max {
        return Err(PersistError::Corrupt(format!(
            "{name}: dimensione dichiarata {} oltre il limite {max}",
            entry.size()
        )));
    }
    let mut buf = Vec::new();
    entry.take(max + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > max {
        return Err(PersistError::Corrupt(format!(
            "{name}: dimensione effettiva oltre il limite {max}"
        )));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::fixtures::sample_room;
    use std::path::PathBuf;

    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new(tag: &str) -> TmpDir {
            let p = std::env::temp_dir().join(format!("waku-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
        fn file(&self, n: &str) -> PathBuf {
            self.0.join(n)
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MANIFEST_V1: &str =
        r#"{"format":"waku","version":1,"app":"t","created":"x","model":"model.json"}"#;

    fn write_zip(path: &Path, manifest: &str, model: &[u8]) {
        let mut zw = ZipWriter::new(File::create(path).unwrap());
        let o = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zw.start_file(MANIFEST_NAME, o).unwrap();
        zw.write_all(manifest.as_bytes()).unwrap();
        zw.start_file(MODEL_NAME, o).unwrap();
        zw.write_all(model).unwrap();
        zw.finish().unwrap();
    }

    #[test]
    fn roundtrip_equal() {
        let d = TmpDir::new("rt");
        let (m, _) = sample_room();
        let p = d.file("a.waku");
        save(&p, &m).unwrap();
        assert_eq!(load(&p).unwrap(), m);
        assert!(!atomic::tmp_path(&p).exists());
    }

    #[test]
    fn save_overwrites_existing() {
        let d = TmpDir::new("ow");
        let (m, _) = sample_room();
        let p = d.file("a.waku");
        save(&p, &m).unwrap();
        save(&p, &m).unwrap();
        assert_eq!(load(&p).unwrap(), m);
    }

    #[test]
    fn failed_write_keeps_original() {
        let d = TmpDir::new("fw");
        let (m, _) = sample_room();
        let p = d.file("a.waku");
        save(&p, &m).unwrap();
        let before = std::fs::read(&p).unwrap();
        let r = save_with(&p, &m, |tmp, _| {
            let mut f = File::create(tmp)?;
            f.write_all(b"PK-meta-scritto")?;
            Err(io::Error::other("disco pieno simulato").into())
        });
        assert!(matches!(r, Err(PersistError::Io(_))));
        assert_eq!(std::fs::read(&p).unwrap(), before);
        assert!(!atomic::tmp_path(&p).exists());
        assert_eq!(load(&p).unwrap(), m);
    }

    #[test]
    fn future_version_rejected() {
        let d = TmpDir::new("fv");
        let (m, _) = sample_room();
        let p = d.file("f.waku");
        let manifest =
            r#"{"format":"waku","version":2,"app":"9","created":"x","model":"model.json"}"#;
        write_zip(&p, manifest, &serde_json::to_vec(&m).unwrap());
        let before = std::fs::read(&p).unwrap();
        assert!(matches!(
            load(&p),
            Err(PersistError::FutureVersion {
                found: 2,
                supported: 1
            })
        ));
        assert_eq!(std::fs::read(&p).unwrap(), before);
        assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 1);
    }

    #[test]
    fn zip_bomb_rejected() {
        let d = TmpDir::new("zb");
        let (m, _) = sample_room();
        let p = d.file("b.waku");
        save(&p, &m).unwrap();
        let tight = Limits {
            max_entry_bytes: 64,
            max_entries: MAX_ENTRIES,
        };
        assert!(matches!(
            load_with_limits(&p, tight),
            Err(PersistError::Corrupt(_))
        ));
        let few = Limits {
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_entries: 1,
        };
        assert!(matches!(
            load_with_limits(&p, few),
            Err(PersistError::Corrupt(_))
        ));
    }

    #[test]
    fn load_clamps_next_id() {
        let d = TmpDir::new("ni");
        let (m, ids) = sample_room();
        let mut j = serde_json::to_value(&m).unwrap();
        j["next_id"] = 0.into();
        let p = d.file("n.waku");
        write_zip(&p, MANIFEST_V1, &serde_json::to_vec(&j).unwrap());
        let mut loaded = load(&p).unwrap();
        let n = loaded.alloc_id();
        assert!(n > ids.door && n > ids.walls[3]);
    }

    #[test]
    fn invalid_model_is_corrupt() {
        let d = TmpDir::new("iv");
        let (m, ids) = sample_room();
        let mut j = serde_json::to_value(&m).unwrap();
        j["walls"][ids.walls[0].get().to_string()]["thickness"] = serde_json::json!(0);
        let p = d.file("i.waku");
        write_zip(&p, MANIFEST_V1, &serde_json::to_vec(&j).unwrap());
        assert!(matches!(load(&p), Err(PersistError::Corrupt(_))));
    }

    #[test]
    fn second_lock_fails() {
        let d = TmpDir::new("lk");
        let p = d.file("a.waku");
        let l = ProjectLock::acquire(&p).unwrap();
        assert!(l.path().exists());
        assert!(matches!(
            ProjectLock::acquire(&p),
            Err(PersistError::Locked)
        ));
        let lp = l.path().to_owned();
        drop(l);
        assert!(!lp.exists());
        assert!(ProjectLock::acquire(&p).is_ok());
    }
}
