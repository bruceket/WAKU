//! Parser a discesa ricorsiva di espressioni dimensionali (nessun eval).
//!
//! Grammatica: expr = term (('+'|'-') term)*; term = unary (('*'|'/') unary)*;
//! unary = ('+'|'-') unary | atom; atom = numero [unità] | '(' expr ')'.

use crate::units::{Length, LengthUnit};
use thiserror::Error;

const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExprError {
    #[error("divisione per zero")]
    DivisionByZero,
    #[error("errore di sintassi alla posizione {pos}")]
    Syntax { pos: usize },
    #[error("risultato non finito")]
    NonFinite,
    #[error("unità incompatibili nell'operazione")]
    UnitMismatch,
}

/// Valore intermedio: scalare (nell'unità di default) oppure lunghezza in mm.
#[derive(Copy, Clone)]
enum Val {
    Scalar(f64),
    Len(f64),
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    default_unit: LengthUnit,
    depth: usize,
}

fn finite(v: f64) -> Result<f64, ExprError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(ExprError::NonFinite)
    }
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.src.get(self.pos).copied()
    }

    fn syntax<T>(&self) -> Result<T, ExprError> {
        Err(ExprError::Syntax { pos: self.pos })
    }

    fn mm(&self, v: Val) -> f64 {
        match v {
            Val::Scalar(s) => s * self.default_unit.mm_factor(),
            Val::Len(l) => l,
        }
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.syntax();
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<Val, ExprError> {
        let mut acc = self.term()?;
        while let Some(op @ (b'+' | b'-')) = self.peek() {
            self.pos += 1;
            let rhs = self.term()?;
            acc = match (acc, rhs) {
                (Val::Scalar(a), Val::Scalar(b)) => {
                    Val::Scalar(finite(if op == b'+' { a + b } else { a - b })?)
                }
                (a, b) => {
                    let (a, b) = (self.mm(a), self.mm(b));
                    Val::Len(finite(if op == b'+' { a + b } else { a - b })?)
                }
            };
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<Val, ExprError> {
        let mut acc = self.unary()?;
        while let Some(op @ (b'*' | b'/')) = self.peek() {
            self.pos += 1;
            let rhs = self.unary()?;
            if op == b'/' {
                let zero = match rhs {
                    Val::Scalar(v) | Val::Len(v) => v == 0.0,
                };
                if zero {
                    return Err(ExprError::DivisionByZero);
                }
            }
            acc = match (acc, rhs, op) {
                (Val::Scalar(a), Val::Scalar(b), b'*') => Val::Scalar(finite(a * b)?),
                (Val::Scalar(a), Val::Scalar(b), _) => Val::Scalar(finite(a / b)?),
                (Val::Len(a), Val::Scalar(b), b'*') | (Val::Scalar(b), Val::Len(a), b'*') => {
                    Val::Len(finite(a * b)?)
                }
                (Val::Len(a), Val::Scalar(b), _) => Val::Len(finite(a / b)?),
                _ => return Err(ExprError::UnitMismatch),
            };
        }
        Ok(acc)
    }

    fn unary(&mut self) -> Result<Val, ExprError> {
        match self.peek() {
            Some(op @ (b'+' | b'-')) => {
                self.pos += 1;
                self.enter()?;
                let v = self.unary();
                self.depth -= 1;
                let v = v?;
                Ok(if op == b'-' {
                    match v {
                        Val::Scalar(s) => Val::Scalar(-s),
                        Val::Len(l) => Val::Len(-l),
                    }
                } else {
                    v
                })
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<Val, ExprError> {
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                self.enter()?;
                let v = self.expr();
                self.depth -= 1;
                let v = v?;
                if self.peek() != Some(b')') {
                    return self.syntax();
                }
                self.pos += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => self.number(),
            _ => self.syntax(),
        }
    }

    fn number(&mut self) -> Result<Val, ExprError> {
        let start = self.pos;
        let s = self.src;
        while self.pos < s.len() && (s[self.pos].is_ascii_digit() || s[self.pos] == b'.') {
            self.pos += 1;
        }
        // Esponente solo se seguito da cifra (o segno + cifra).
        if matches!(s.get(self.pos), Some(b'e' | b'E')) {
            let mut p = self.pos + 1;
            if matches!(s.get(p), Some(b'+' | b'-')) {
                p += 1;
            }
            if s.get(p).is_some_and(u8::is_ascii_digit) {
                while s.get(p).is_some_and(u8::is_ascii_digit) {
                    p += 1;
                }
                self.pos = p;
            }
        }
        let text = std::str::from_utf8(&s[start..self.pos]).unwrap_or("");
        let Ok(n) = text.parse::<f64>() else {
            self.pos = start;
            return self.syntax();
        };
        let n = finite(n)?;
        // Unità opzionale.
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            let us = self.pos;
            while self.pos < s.len() && s[self.pos].is_ascii_alphabetic() {
                self.pos += 1;
            }
            let ident = std::str::from_utf8(&s[us..self.pos]).unwrap_or("");
            return match LengthUnit::from_suffix(ident) {
                Some(u) => Ok(Val::Len(finite(n * u.mm_factor())?)),
                None => {
                    self.pos = us;
                    self.syntax()
                }
            };
        }
        Ok(Val::Scalar(n))
    }
}

/// Valuta un'espressione dimensionale. I numeri senza unità usano `default_unit`.
pub fn parse_length(expr: &str, default_unit: LengthUnit) -> Result<Length, ExprError> {
    let mut p = Parser {
        src: expr.as_bytes(),
        pos: 0,
        default_unit,
        depth: 0,
    };
    let v = p.expr()?;
    if p.peek().is_some() {
        return p.syntax();
    }
    Ok(Length::mm(finite(p.mm(v))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::*;
    use proptest::prelude::*;

    #[test]
    fn ac01_subtraction_cm() {
        assert_eq!(
            parse_length("332-60", LengthUnit::Cm)
                .unwrap()
                .to_unit(LengthUnit::Cm),
            272.0
        );
    }
    #[test]
    fn division() {
        assert_eq!(
            parse_length("550/2", LengthUnit::Mm).unwrap().as_mm(),
            275.0
        );
    }
    #[test]
    fn explicit_unit() {
        assert_eq!(
            parse_length("2.5 m", LengthUnit::Cm).unwrap().as_mm(),
            2500.0
        );
    }
    #[test]
    fn mixed_units() {
        assert_eq!(
            parse_length("2 m + 30 cm", LengthUnit::Mm).unwrap().as_mm(),
            2300.0
        );
    }
    #[test]
    fn parens_mul() {
        assert_eq!(
            parse_length("(100+20)*2", LengthUnit::Mm).unwrap().as_mm(),
            240.0
        );
    }
    #[test]
    fn div_zero() {
        assert_eq!(
            parse_length("5/0", LengthUnit::Mm),
            Err(ExprError::DivisionByZero)
        );
    }
    #[test]
    fn overflow() {
        assert_eq!(
            parse_length("1e400", LengthUnit::Mm),
            Err(ExprError::NonFinite)
        );
    }
    #[test]
    fn garbage() {
        assert!(matches!(
            parse_length("abc", LengthUnit::Mm),
            Err(ExprError::Syntax { .. })
        ));
    }
    #[test]
    fn unit_mismatch() {
        assert_eq!(
            parse_length("2 m * 3 m", LengthUnit::Mm),
            Err(ExprError::UnitMismatch)
        );
    }
    #[test]
    fn mul_one_unit() {
        assert_eq!(
            parse_length("2 m * 3", LengthUnit::Mm).unwrap().as_mm(),
            6000.0
        );
    }
    #[test]
    fn unary_minus() {
        assert_eq!(parse_length("-5+10", LengthUnit::Mm).unwrap().as_mm(), 5.0);
    }
    #[test]
    fn empty_and_trailing() {
        assert!(matches!(
            parse_length("", LengthUnit::Mm),
            Err(ExprError::Syntax { .. })
        ));
        assert_eq!(
            parse_length("1+", LengthUnit::Mm),
            Err(ExprError::Syntax { pos: 2 })
        );
        assert!(matches!(
            parse_length("1 2", LengthUnit::Mm),
            Err(ExprError::Syntax { .. })
        ));
        assert!(matches!(
            parse_length("(1", LengthUnit::Mm),
            Err(ExprError::Syntax { .. })
        ));
        assert!(matches!(
            parse_length("3 km", LengthUnit::Mm),
            Err(ExprError::Syntax { .. })
        ));
    }
    #[test]
    fn deep_nesting_no_overflow() {
        let s = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_length(&s, LengthUnit::Mm).is_err());
    }

    proptest! {
        #[test]
        fn add_matches(a in 0u32..1_000_000, b in 0u32..1_000_000) {
            let r = parse_length(&format!("{a}+{b}"), LengthUnit::Mm).unwrap().as_mm();
            prop_assert_eq!(r, f64::from(a) + f64::from(b));
        }
        #[test]
        fn never_panics(s in ".{0,40}") { let _ = parse_length(&s, LengthUnit::Mm); }
    }
}
