//! Fixture di test (feature `fixtures`).

use crate::*;

/// Identificatori degli elementi di `sample_room`.
#[derive(Copy, Clone, Debug)]
pub struct SampleIds {
    pub level: ElementId,
    pub walls: [ElementId; 4],
    pub door: ElementId,
}

/// Stanza 4000x3000 mm: 4 muri sull'asse (spessore 100, altezza 2700) e una
/// porta 800x2100 con offset 500 sul muro 0.
pub fn sample_room() -> (Model, SampleIds) {
    let mut m = Model::new("Stanza di esempio", LengthUnit::Mm);
    let level = *m.levels.keys().next().expect("livello predefinito");
    let corners = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
    let mut walls = [level; 4];
    for i in 0..4 {
        let (sx, sy) = corners[i];
        let (ex, ey) = corners[(i + 1) % 4];
        let id = m.alloc_id();
        m.walls.insert(
            id,
            Wall {
                guid: IfcGuid::new(),
                level,
                start: Point2::new(sx, sy),
                end: Point2::new(ex, ey),
                thickness: Length::mm(100.0),
                height: Length::mm(2700.0),
                location: WallLocation::Axis,
            },
        );
        walls[i] = id;
    }
    let door = m.alloc_id();
    m.openings.insert(
        door,
        Opening {
            guid: IfcGuid::new(),
            host: walls[0],
            kind: OpeningKind::Door,
            offset: Length::mm(500.0),
            width: Length::mm(800.0),
            height: Length::mm(2100.0),
            sill: Length::mm(0.0),
        },
    );
    (m, SampleIds { level, walls, door })
}
