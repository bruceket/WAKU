use domain::fixtures::sample_room;
use domain::{IfcGuid, Model, Space};
use ifc::{ExportOptions, export_ifc4_with};
use std::collections::HashSet;
use std::path::PathBuf;

/// sample_room con GUID deterministici e un vano rilevato.
fn sample() -> Model {
    let (mut m, ids) = sample_room();
    let mut k = 0x1000u128;
    let mut next = || {
        k += 1;
        IfcGuid::from_u128(k)
    };
    m.project.guid = next();
    m.project.site_guid = next();
    m.project.building_guid = next();
    for l in m.levels.values_mut() {
        l.guid = next();
    }
    for w in m.walls.values_mut() {
        w.guid = next();
    }
    for o in m.openings.values_mut() {
        o.guid = next();
    }
    let faces = geometry::detect_spaces(&m, ids.level, 1.0).unwrap();
    assert_eq!(faces.len(), 1);
    let id = m.alloc_id();
    m.spaces.insert(
        id,
        Space {
            guid: next(),
            level: ids.level,
            boundary: faces[0].clone(),
            name: "Soggiorno".to_owned(),
        },
    );
    m
}

fn export(m: &Model) -> String {
    let mut buf = Vec::new();
    let opts = ExportOptions {
        timestamp: "2026-11-17T00:00:00".to_owned(),
        file_name: "sample_room.ifc".to_owned(),
    };
    let rep = export_ifc4_with(m, &mut buf, &opts).unwrap();
    assert!(rep.skipped.is_empty(), "{:?}", rep.skipped);
    String::from_utf8(buf).unwrap()
}

fn all_guids(m: &Model) -> Vec<String> {
    let mut v = vec![
        m.project.guid.as_str().to_owned(),
        m.project.site_guid.as_str().to_owned(),
        m.project.building_guid.as_str().to_owned(),
    ];
    v.extend(m.levels.values().map(|x| x.guid.as_str().to_owned()));
    v.extend(m.walls.values().map(|x| x.guid.as_str().to_owned()));
    v.extend(m.openings.values().map(|x| x.guid.as_str().to_owned()));
    v.extend(m.spaces.values().map(|x| x.guid.as_str().to_owned()));
    v
}

#[test]
fn golden_sample_room() {
    let out = export(&sample());
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/g0/sample_room.ifc");
    if std::env::var("WAKU_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, &out).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("golden mancante: rigenerare con WAKU_BLESS=1")
        .replace("\r\n", "\n");
    assert_eq!(
        out, golden,
        "output diverso dal golden (WAKU_BLESS=1 per aggiornare)"
    );
}

#[test]
fn deterministic() {
    let m = sample();
    assert_eq!(export(&m), export(&m));
}

#[test]
fn guids_preserved() {
    let m = sample();
    let out = export(&m);
    for g in all_guids(&m) {
        let quoted = format!("'{g}'");
        assert_eq!(out.matches(&quoted).count(), 1, "GUID {g}");
    }
}

#[test]
fn door_voids_and_fills() {
    let out = export(&sample());
    assert_eq!(out.matches("=IFCRELVOIDSELEMENT(").count(), 1);
    assert_eq!(out.matches("=IFCRELFILLSELEMENT(").count(), 1);
    assert_eq!(out.matches("=IFCDOOR(").count(), 1);
    assert_eq!(out.matches("=IFCOPENINGELEMENT(").count(), 1);
    assert_eq!(out.matches("=IFCWALL(").count(), 4);
    assert_eq!(out.matches("=IFCSPACE(").count(), 1);
    assert_eq!(out.matches("=IFCBUILDINGSTOREY(").count(), 1);
}

#[test]
fn header_and_units() {
    let out = export(&sample());
    assert!(out.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(out.contains("FILE_SCHEMA(('IFC4'));"));
    assert!(out.contains("'2026-11-17T00:00:00'"));
    assert!(out.contains("IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)"));
    assert!(out.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
}

#[test]
fn skips_invalid_wall_and_hosted_door() {
    let mut m = sample();
    let (&wid, _) = m.walls.iter().next().unwrap();
    let w = m.walls.get_mut(&wid).unwrap();
    w.end = w.start;
    let mut buf = Vec::new();
    let rep = ifc::export_ifc4(&m, &mut buf).unwrap();
    assert!(rep.skipped.iter().any(|(id, _)| *id == wid));
    assert_eq!(rep.skipped.len(), 3, "{:?}", rep.skipped); // muro, porta, vano
}

/// Parser STEP minimale: ogni riga DATA e' `#n=NOME(...);` bilanciata fuori
/// dalle stringhe, id univoci, ogni `#k` referenziato esiste.
#[test]
fn parses_as_step() {
    let out = export(&sample());
    let data = out
        .split_once("DATA;\n")
        .unwrap()
        .1
        .split_once("ENDSEC;\nEND-ISO-10303-21;")
        .unwrap()
        .0;
    let mut ids = HashSet::new();
    let mut refs = HashSet::new();
    for line in data.lines() {
        let (lhs, rhs) = line.split_once('=').expect("riga senza '='");
        let id: u64 = lhs.strip_prefix('#').unwrap().parse().unwrap();
        assert!(ids.insert(id), "id duplicato #{id}");
        let name = &rhs[..rhs.find('(').unwrap()];
        assert!(
            name.starts_with("IFC")
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{line}"
        );
        assert!(rhs.ends_with(");"), "{line}");
        let b = rhs.as_bytes();
        let (mut depth, mut in_str, mut i) = (0i32, false, 0);
        while i < b.len() {
            let c = b[i];
            assert!(c.is_ascii() && c >= 0x20, "{line}");
            if in_str {
                if c == b'\'' {
                    if b.get(i + 1) == Some(&b'\'') {
                        i += 1;
                    } else {
                        in_str = false;
                    }
                }
            } else {
                match c {
                    b'\'' => in_str = true,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        assert!(depth >= 0, "{line}");
                    }
                    b'#' => {
                        let n: String = rhs[i + 1..]
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        refs.insert(n.parse::<u64>().unwrap());
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        assert!(!in_str && depth == 0, "non bilanciata: {line}");
    }
    for r in &refs {
        assert!(ids.contains(r), "riferimento pendente #{r}");
    }
}
