//! Token di sessione del bridge.
//!
//! Limite G0: il file token non riceve una DACL esplicita; si affida all'ACL
//! per-utente ereditata da `%LOCALAPPDATA%` (accessibile solo all'utente, a
//! SYSTEM e agli amministratori).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub const TOKEN_FILE: &str = "session.token";

/// Token segreto di 32 byte, rappresentato in esadecimale.
#[derive(Clone)]
pub struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(***)")
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Token {
    /// 32 byte casuali dal generatore del sistema operativo.
    pub fn generate() -> io::Result<Self> {
        let mut buf = [0u8; 32];
        getrandom::fill(&mut buf).map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Token(hex(&buf)))
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim();
        (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Token(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Confronto a tempo costante (dipende solo dalle lunghezze).
    pub fn matches(&self, candidate: &str) -> bool {
        let a = self.0.as_bytes();
        let b = candidate.as_bytes();
        let mut diff = a.len() ^ b.len();
        for (i, x) in a.iter().enumerate() {
            diff |= (x ^ b.get(i).copied().unwrap_or(0)) as usize;
        }
        diff == 0
    }
}

/// Directory predefinita: `%LOCALAPPDATA%\Waku`.
pub fn default_token_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Waku"))
}

/// Scrive `<dir>/session.token` (crea la directory se manca).
pub fn write_token_file(dir: &Path, token: &Token) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(TOKEN_FILE);
    std::fs::write(&p, token.as_str())?;
    Ok(p)
}

pub fn read_token_file(dir: &Path) -> io::Result<Token> {
    let s = std::fs::read_to_string(dir.join(TOKEN_FILE))?;
    Token::from_hex(&s)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "token non valido"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_64_hex_and_unique() {
        let a = Token::generate().unwrap();
        let b = Token::generate().unwrap();
        assert_eq!(a.as_str().len(), 64);
        assert_ne!(a.as_str(), b.as_str());
        assert!(a.matches(a.as_str()));
        assert!(!a.matches(b.as_str()));
        assert!(!a.matches(""));
        assert!(!a.matches(&format!("{}0", a.as_str())));
    }

    #[test]
    fn roundtrip_file() {
        let dir = std::env::temp_dir().join(format!("waku-token-{}", std::process::id()));
        let t = Token::generate().unwrap();
        let p = write_token_file(&dir, &t).unwrap();
        assert!(p.ends_with(TOKEN_FILE));
        assert_eq!(read_token_file(&dir).unwrap().as_str(), t.as_str());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
