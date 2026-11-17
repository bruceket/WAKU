//! Unità di lunghezza. Rappresentazione interna: millimetri.

use serde::{Deserialize, Serialize};

/// Unità di lunghezza supportate.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Mm,
    Cm,
    M,
}

impl LengthUnit {
    /// Suffisso testuale (`mm`, `cm`, `m`).
    pub fn suffix(self) -> &'static str {
        match self {
            LengthUnit::Mm => "mm",
            LengthUnit::Cm => "cm",
            LengthUnit::M => "m",
        }
    }

    /// Da suffisso testuale.
    pub fn from_suffix(s: &str) -> Option<Self> {
        match s {
            "mm" => Some(LengthUnit::Mm),
            "cm" => Some(LengthUnit::Cm),
            "m" => Some(LengthUnit::M),
            _ => None,
        }
    }

    /// Millimetri per unità.
    pub fn mm_factor(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Cm => 10.0,
            LengthUnit::M => 1000.0,
        }
    }
}

/// Lunghezza, memorizzata in millimetri.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Length(f64);

impl Length {
    pub fn mm(v: f64) -> Self {
        Length(v)
    }

    pub fn from_unit(v: f64, unit: LengthUnit) -> Self {
        Length(v * unit.mm_factor())
    }

    pub fn as_mm(self) -> f64 {
        self.0
    }

    pub fn to_unit(self, unit: LengthUnit) -> f64 {
        self.0 / unit.mm_factor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversions() {
        assert_eq!(Length::from_unit(2.0, LengthUnit::M).as_mm(), 2000.0);
        assert_eq!(Length::mm(30.0).to_unit(LengthUnit::Cm), 3.0);
        assert_eq!(Length::from_unit(1.0, LengthUnit::Cm).as_mm(), 10.0);
    }
}
