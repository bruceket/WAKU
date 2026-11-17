//! Strumento muro: primo click = inizio, secondo click (o Invio sul campo
//! lunghezza) = fine. Lunghezza, spessore e altezza accettano espressioni.

use crate::app::length_field_commit;
use domain::LengthUnit;

/// Strumento attivo nella vista pianta.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Wall,
    Door,
}

/// Muro pronto per `CreateWall`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WallDraft {
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub thickness_mm: f64,
    pub height_mm: f64,
}

/// Stato dello strumento muro.
#[derive(Clone, Debug)]
pub struct WallTool {
    pub start: Option<[f64; 2]>,
    pub length_text: String,
    pub thickness_text: String,
    pub height_text: String,
}

impl Default for WallTool {
    fn default() -> Self {
        Self {
            start: None,
            length_text: String::new(),
            thickness_text: "100 mm".to_owned(),
            height_text: "2700 mm".to_owned(),
        }
    }
}

/// Punto finale: lungo la direzione `start -> cursor`, alla lunghezza data se presente.
pub fn wall_end(start: [f64; 2], cursor: [f64; 2], length_mm: Option<f64>) -> [f64; 2] {
    let (dx, dy) = (cursor[0] - start[0], cursor[1] - start[1]);
    let d = dx.hypot(dy);
    match length_mm {
        Some(l) if d > f64::EPSILON => [start[0] + dx / d * l, start[1] + dy / d * l],
        // Senza direzione: asse X positivo.
        Some(l) => [start[0] + l, start[1]],
        None => cursor,
    }
}

impl WallTool {
    pub fn drawing(&self) -> bool {
        self.start.is_some()
    }

    pub fn cancel(&mut self) {
        self.start = None;
        self.length_text.clear();
    }

    /// Lunghezza digitata (se il campo non e' vuoto).
    fn typed_length(&self, unit: LengthUnit) -> Result<Option<f64>, String> {
        if self.length_text.trim().is_empty() {
            Ok(None)
        } else {
            length_field_commit(&self.length_text, unit).map(|l| Some(l.as_mm()))
        }
    }

    /// Anteprima del muro in corso.
    pub fn preview(&self, cursor: [f64; 2], unit: LengthUnit) -> Option<([f64; 2], [f64; 2])> {
        let start = self.start?;
        let len = self.typed_length(unit).ok().flatten();
        Some((start, wall_end(start, cursor, len)))
    }

    /// Chiude il muro verso `cursor`; in caso di errore lo stato resta invariato.
    pub fn commit(&mut self, cursor: [f64; 2], unit: LengthUnit) -> Result<WallDraft, String> {
        let start = self.start.ok_or("nessun punto iniziale")?;
        let len = self.typed_length(unit)?;
        let thickness_mm = length_field_commit(&self.thickness_text, unit)
            .map_err(|e| format!("spessore: {e}"))?
            .as_mm();
        let height_mm = length_field_commit(&self.height_text, unit)
            .map_err(|e| format!("altezza: {e}"))?
            .as_mm();
        let end = wall_end(start, cursor, len);
        self.cancel();
        Ok(WallDraft {
            start,
            end,
            thickness_mm,
            height_mm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_end_uses_typed_length_along_cursor_direction() {
        assert_eq!(
            wall_end([0.0, 0.0], [10.0, 0.0], Some(2720.0)),
            [2720.0, 0.0]
        );
        assert_eq!(wall_end([0.0, 0.0], [3.0, 4.0], None), [3.0, 4.0]);
        let e = wall_end([0.0, 0.0], [0.0, -5.0], Some(100.0));
        assert!((e[1] + 100.0).abs() < 1e-9);
    }

    #[test]
    fn commit_with_expression_and_defaults() {
        let mut t = WallTool {
            start: Some([0.0, 0.0]),
            length_text: "332-60".to_owned(),
            ..Default::default()
        };
        let d = t.commit([1.0, 0.0], LengthUnit::Cm).unwrap();
        assert_eq!(d.end, [2720.0, 0.0]);
        assert_eq!(d.thickness_mm, 100.0);
        assert_eq!(d.height_mm, 2700.0);
        assert!(!t.drawing());
    }

    #[test]
    fn commit_error_keeps_state() {
        let mut t = WallTool {
            start: Some([0.0, 0.0]),
            length_text: "abc".to_owned(),
            ..Default::default()
        };
        assert!(t.commit([1.0, 0.0], LengthUnit::Mm).is_err());
        assert!(t.drawing());
    }
}
