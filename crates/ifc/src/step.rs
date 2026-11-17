//! Primitive di serializzazione STEP (ISO 10303-21).

use std::fmt::Write as _;

/// Reale formattato in modo deterministico: 6 decimali senza zeri finali,
/// punto decimale sempre presente (`4000.`, `0.5`); `-0` diventa `0.`.
pub fn real(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    if s == "-0." {
        "0.".to_owned()
    } else {
        s.to_owned()
    }
}

/// Stringa STEP tra apici: apostrofo e backslash raddoppiati; caratteri non
/// ASCII stampabili codificati come `\X2\hhhh..\X0\` (BMP, 4 hex) o
/// `\X4\hhhhhhhh..\X0\` (fuori BMP, 8 hex), hex maiuscolo.
pub fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    // 0 = testo semplice, 2 = blocco \X2\, 4 = blocco \X4\.
    let mut mode = 0u8;
    for c in s.chars() {
        let want = if (' '..='~').contains(&c) {
            0
        } else if (c as u32) <= 0xFFFF {
            2
        } else {
            4
        };
        if want != mode {
            if mode != 0 {
                out.push_str("\\X0\\");
            }
            match want {
                2 => out.push_str("\\X2\\"),
                4 => out.push_str("\\X4\\"),
                _ => {}
            }
            mode = want;
        }
        match (mode, c) {
            (0, '\'') => out.push_str("''"),
            (0, '\\') => out.push_str("\\\\"),
            (0, c) => out.push(c),
            (2, c) => {
                let _ = write!(out, "{:04X}", c as u32);
            }
            (_, c) => {
                let _ = write!(out, "{:08X}", c as u32);
            }
        }
    }
    if mode != 0 {
        out.push_str("\\X0\\");
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals() {
        assert_eq!(real(4000.0), "4000.");
        assert_eq!(real(0.5), "0.5");
        assert_eq!(real(-0.0), "0.");
        assert_eq!(real(0.0), "0.");
        assert_eq!(real(-12.25), "-12.25");
        assert_eq!(real(1.0 / 3.0), "0.333333");
        assert_eq!(real(1e-5), "0.00001");
        assert_eq!(real(-1e-9), "0.");
    }

    #[test]
    fn strings() {
        assert_eq!(string("Piano terra"), "'Piano terra'");
        assert_eq!(string("l'atrio"), "'l''atrio'");
        assert_eq!(string("a\\b"), "'a\\\\b'");
        assert_eq!(string("città"), "'citt\\X2\\00E0\\X0\\'");
        assert_eq!(string("è€x"), "'\\X2\\00E820AC\\X0\\x'");
        assert_eq!(string("😀"), "'\\X4\\0001F600\\X0\\'");
        assert_eq!(string("a\nb"), "'a\\X2\\000A\\X0\\b'");
    }
}
