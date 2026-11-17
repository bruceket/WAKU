//! Riepilogo JSON del modello (condiviso da CLI e risorsa `waku://model/summary`).

use crate::Snapshot;
use serde_json::{Value, json};

/// Riepilogo: `{revision, project, levels, walls, openings, spaces}`.
pub fn model_summary(snap: &Snapshot) -> Value {
    let m = &snap.model;
    json!({
        "revision": snap.revision.0,
        "project": m.project.name,
        "levels": m.levels.iter()
            .map(|(id, l)| json!({"id": id.get(), "name": l.name}))
            .collect::<Vec<_>>(),
        "walls": m.walls.len(),
        "openings": m.openings.len(),
        "spaces": m.spaces.iter()
            .map(|(id, s)| json!({
                "id": id.get(),
                "name": s.name,
                "area_m2": geometry::space_area(m, &s.boundary) / 1e6,
            }))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Command, CommandService, Origin, RequestId, Revision};
    use domain::fixtures::sample_room;

    #[test]
    fn summary_of_sample_room() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(Command {
            request_id: RequestId("s".into()),
            expected: Revision(0),
            origin: Origin::Cli,
            actions: vec![Action::DetectSpaces { level: ids.level }],
            preview: false,
        })
        .unwrap();
        let v = model_summary(&svc.snapshot());
        assert_eq!(v["revision"], 1);
        assert_eq!(v["project"], "Stanza di esempio");
        assert_eq!(v["levels"][0]["name"], "Piano terra");
        assert_eq!(v["walls"], 4);
        assert_eq!(v["openings"], 1);
        assert_eq!(v["spaces"].as_array().unwrap().len(), 1);
        let a = v["spaces"][0]["area_m2"].as_f64().unwrap();
        assert!((a - 11.31).abs() < 1e-6, "{a}");
    }
}
