//! Identificatori: ElementId e IfcGuid (compressione base64 IFC a 22 caratteri).

use serde::{Deserialize, Serialize};
use thiserror::Error;

const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// Identificatore interno di un elemento del modello.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ElementId(u64);

impl ElementId {
    pub fn new(v: u64) -> Self {
        ElementId(v)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Errori di parsing di un GUID IFC.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GuidError {
    #[error("lunghezza GUID IFC non valida: attesi 22 caratteri, trovati {0}")]
    Length(usize),
    #[error("carattere non valido nel GUID IFC alla posizione {0}")]
    InvalidChar(usize),
    #[error("GUID IFC fuori dal range a 128 bit")]
    Overflow,
}

/// GUID IFC: UUID a 128 bit codificato in 22 caratteri.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IfcGuid(String);

impl TryFrom<String> for IfcGuid {
    type Error = GuidError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        IfcGuid::parse(&s)
    }
}

impl From<IfcGuid> for String {
    fn from(g: IfcGuid) -> String {
        g.0
    }
}

impl IfcGuid {
    /// Nuovo GUID casuale (UUID v4).
    pub fn new() -> Self {
        Self::from_u128(uuid::Uuid::new_v4().as_u128())
    }

    /// Codifica un valore a 128 bit: il primo carattere porta i 2 bit alti,
    /// i 21 successivi 6 bit ciascuno.
    pub fn from_u128(v: u128) -> Self {
        let mut out = String::with_capacity(22);
        out.push(ALPHABET[(v >> 126) as usize] as char);
        for k in 0..21 {
            let shift = 120 - 6 * k;
            out.push(ALPHABET[((v >> shift) & 63) as usize] as char);
        }
        IfcGuid(out)
    }

    /// Decodifica nel valore a 128 bit.
    pub fn to_u128(&self) -> u128 {
        self.0.bytes().fold(0u128, |acc, b| {
            let d = ALPHABET.iter().position(|&c| c == b).unwrap_or(0) as u128;
            (acc << 6) | d
        })
    }

    pub fn parse(s: &str) -> Result<Self, GuidError> {
        let n = s.chars().count();
        if n != 22 {
            return Err(GuidError::Length(n));
        }
        for (i, c) in s.chars().enumerate() {
            if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
                return Err(GuidError::InvalidChar(i));
            }
        }
        // Il primo carattere codifica solo 2 bit: valori > 3 eccedono 128 bit.
        let first = ALPHABET
            .iter()
            .position(|&c| c == s.as_bytes()[0])
            .unwrap_or(0);
        if first > 3 {
            return Err(GuidError::Overflow);
        }
        Ok(IfcGuid(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for IfcGuid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guid_roundtrip() {
        let g = IfcGuid::new();
        assert_eq!(g.as_str().len(), 22);
        assert_eq!(IfcGuid::parse(g.as_str()).unwrap(), g);
    }
    #[test]
    fn guid_known_vectors() {
        assert_eq!(IfcGuid::from_u128(0).as_str(), "0000000000000000000000");
        assert_eq!(
            IfcGuid::from_u128(u128::MAX).as_str(),
            "3$$$$$$$$$$$$$$$$$$$$$"
        );
        assert_eq!(IfcGuid::from_u128(1).as_str(), "0000000000000000000001");
    }
    #[test]
    fn guid_decode_known() {
        assert_eq!(
            IfcGuid::parse("3$$$$$$$$$$$$$$$$$$$$$").unwrap().to_u128(),
            u128::MAX
        );
    }
    #[test]
    fn guid_rejects_bad() {
        assert!(IfcGuid::parse("short").is_err());
        assert!(IfcGuid::parse("0000000000000000000000x ").is_err());
        assert!(IfcGuid::parse("4000000000000000000000").is_err()); // overflow 128 bit
        assert!(IfcGuid::parse("00000000000000000000!0").is_err());
    }
    #[test]
    fn guid_deserialize_rejects_invalid() {
        assert!(serde_json::from_str::<IfcGuid>("\"abc\"").is_err());
        let g = IfcGuid::new();
        let j = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<IfcGuid>(&j).unwrap(), g);
    }
    #[test]
    fn element_id_ord() {
        assert!(ElementId::new(1) < ElementId::new(2));
        assert_eq!(ElementId::new(7).get(), 7);
    }
}
