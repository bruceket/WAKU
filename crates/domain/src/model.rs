//! Modello BIM G0 e relative invarianti.

use crate::{ElementId, IfcGuid, Length, LengthUnit, Opening, Space, Wall};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub guid: IfcGuid,
    pub name: String,
    pub unit: LengthUnit,
    pub site_guid: IfcGuid,
    pub building_guid: IfcGuid,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub guid: IfcGuid,
    pub name: String,
    pub elevation: Length,
    pub height: Length,
}

/// Errori di validazione del modello.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    #[error("muro {0:?} di lunghezza nulla")]
    ZeroLengthWall(ElementId),
    #[error("valore non positivo per {field} dell'elemento {id:?}")]
    NonPositive { id: ElementId, field: &'static str },
    #[error("apertura {0:?} fuori dai limiti del muro ospite")]
    OpeningOutOfHost(ElementId),
    #[error("elemento {0:?} con muro ospite inesistente")]
    DanglingHost(ElementId),
    #[error("elemento {0:?} con livello inesistente")]
    DanglingLevel(ElementId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub project: Project,
    pub levels: BTreeMap<ElementId, Level>,
    pub walls: BTreeMap<ElementId, Wall>,
    pub openings: BTreeMap<ElementId, Opening>,
    pub spaces: BTreeMap<ElementId, Space>,
    next_id: u64,
}

impl Model {
    /// Nuovo modello con un livello "Piano terra" a quota 0.
    pub fn new(name: &str, unit: LengthUnit) -> Model {
        let mut m = Model {
            project: Project {
                guid: IfcGuid::new(),
                name: name.to_owned(),
                unit,
                site_guid: IfcGuid::new(),
                building_guid: IfcGuid::new(),
            },
            levels: BTreeMap::new(),
            walls: BTreeMap::new(),
            openings: BTreeMap::new(),
            spaces: BTreeMap::new(),
            next_id: 1,
        };
        let id = m.alloc_id();
        m.levels.insert(
            id,
            Level {
                guid: IfcGuid::new(),
                name: "Piano terra".to_owned(),
                elevation: Length::mm(0.0),
                height: Length::mm(3000.0),
            },
        );
        m
    }

    /// Alloca un nuovo identificatore univoco.
    pub fn alloc_id(&mut self) -> ElementId {
        let id = ElementId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Verifica le invarianti del modello.
    pub fn validate(&self) -> Result<(), ModelError> {
        for (&id, l) in &self.levels {
            positive(id, "height", l.height)?;
        }
        for (&id, w) in &self.walls {
            if !self.levels.contains_key(&w.level) {
                return Err(ModelError::DanglingLevel(id));
            }
            if w.length().as_mm() <= 0.0 {
                return Err(ModelError::ZeroLengthWall(id));
            }
            positive(id, "thickness", w.thickness)?;
            positive(id, "height", w.height)?;
        }
        for (&id, o) in &self.openings {
            let wall = self
                .walls
                .get(&o.host)
                .ok_or(ModelError::DanglingHost(id))?;
            positive(id, "width", o.width)?;
            positive(id, "height", o.height)?;
            if o.sill.as_mm().is_nan() || o.sill.as_mm() < 0.0 {
                return Err(ModelError::NonPositive { id, field: "sill" });
            }
            if o.offset.as_mm() < 0.0
                || o.offset.as_mm() + o.width.as_mm() > wall.length().as_mm()
                || o.sill.as_mm() + o.height.as_mm() > wall.height.as_mm()
            {
                return Err(ModelError::OpeningOutOfHost(id));
            }
        }
        for (&id, s) in &self.spaces {
            if !self.levels.contains_key(&s.level) {
                return Err(ModelError::DanglingLevel(id));
            }
            if s.boundary.iter().any(|w| !self.walls.contains_key(w)) {
                return Err(ModelError::DanglingHost(id));
            }
        }
        Ok(())
    }
}

/// Richiede un valore > 0 (respinge anche NaN).
fn positive(id: ElementId, field: &'static str, v: Length) -> Result<(), ModelError> {
    if v.as_mm() > 0.0 {
        Ok(())
    } else {
        Err(ModelError::NonPositive { id, field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point2;
    use crate::fixtures::sample_room;

    #[test]
    fn valid_room_ok() {
        let (m, ids) = sample_room();
        assert_eq!(m.validate(), Ok(()));
        assert_eq!(m.walls.len(), 4);
        assert!(m.levels.contains_key(&ids.level));
        assert!(m.openings.contains_key(&ids.door));
    }

    #[test]
    fn wall_zero_length_invalid() {
        let (mut m, ids) = sample_room();
        let w = m.walls.get_mut(&ids.walls[0]).unwrap();
        w.end = w.start;
        assert_eq!(m.validate(), Err(ModelError::ZeroLengthWall(ids.walls[0])));
    }

    #[test]
    fn thickness_zero_invalid() {
        let (mut m, ids) = sample_room();
        m.walls.get_mut(&ids.walls[1]).unwrap().thickness = Length::mm(0.0);
        assert_eq!(
            m.validate(),
            Err(ModelError::NonPositive {
                id: ids.walls[1],
                field: "thickness"
            })
        );
    }

    #[test]
    fn opening_wider_than_wall_invalid() {
        let (mut m, ids) = sample_room();
        let w = m.walls.get_mut(&ids.walls[0]).unwrap();
        w.end = Point2::new(1000.0, 0.0);
        let o = m.openings.get_mut(&ids.door).unwrap();
        o.offset = Length::mm(500.0);
        o.width = Length::mm(600.0);
        assert_eq!(m.validate(), Err(ModelError::OpeningOutOfHost(ids.door)));
    }

    #[test]
    fn opening_negative_offset_invalid() {
        let (mut m, ids) = sample_room();
        m.openings.get_mut(&ids.door).unwrap().offset = Length::mm(-1.0);
        assert_eq!(m.validate(), Err(ModelError::OpeningOutOfHost(ids.door)));
    }

    #[test]
    fn opening_taller_than_wall_invalid() {
        let (mut m, ids) = sample_room();
        m.openings.get_mut(&ids.door).unwrap().height = Length::mm(2800.0);
        assert_eq!(m.validate(), Err(ModelError::OpeningOutOfHost(ids.door)));
    }

    #[test]
    fn opening_dangling_host_invalid() {
        let (mut m, ids) = sample_room();
        m.walls.remove(&ids.walls[0]);
        assert_eq!(m.validate(), Err(ModelError::DanglingHost(ids.door)));
    }

    #[test]
    fn wall_dangling_level_invalid() {
        let (mut m, ids) = sample_room();
        m.levels.remove(&ids.level);
        assert!(matches!(m.validate(), Err(ModelError::DanglingLevel(_))));
    }

    #[test]
    fn alloc_id_unique() {
        let mut m = Model::new("P", LengthUnit::Mm);
        let a = m.alloc_id();
        let b = m.alloc_id();
        assert_ne!(a, b);
        assert!(!m.levels.contains_key(&a));
    }

    #[test]
    fn model_serializes() {
        let (m, _) = sample_room();
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("Piano terra"));
    }
}
