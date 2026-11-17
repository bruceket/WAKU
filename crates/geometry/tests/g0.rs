use domain::fixtures::sample_room;
use geometry::*;
use std::collections::HashMap;

fn key(p: [f64; 3]) -> [i64; 3] {
    p.map(|v| (v * 1000.0).round() as i64)
}

fn tri(m: &Mesh, t: usize) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|k| m.positions[m.indices[3 * t + k] as usize])
}

#[test]
fn wall_mesh_without_opening_has_12_triangles() {
    let (m, ids) = sample_room();
    let mesh = wall_mesh(&m, ids.walls[1]).unwrap();
    assert_eq!(mesh.indices.len() / 3, 12);
    assert_eq!(mesh.element, ids.walls[1]);
    assert_eq!(mesh.positions.len(), mesh.normals.len());
}

#[test]
fn wall_mesh_with_door_is_closed() {
    let (m, ids) = sample_room();
    let mesh = wall_mesh(&m, ids.walls[0]).unwrap();
    let mut edges: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();
    for t in 0..mesh.indices.len() / 3 {
        let v = tri(&mesh, t).map(key);
        for (a, b) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
            let e = if a < b { (a, b) } else { (b, a) };
            *edges.entry(e).or_default() += 1;
        }
    }
    assert!(!edges.is_empty());
    for (e, n) in &edges {
        assert_eq!(*n, 2, "lato {e:?} condiviso da {n} triangoli");
    }
}

#[test]
fn wall_mesh_volume() {
    let (m, ids) = sample_room();
    let mesh = wall_mesh(&m, ids.walls[0]).unwrap();
    let mut vol = 0.0;
    for t in 0..mesh.indices.len() / 3 {
        let [a, b, c] = tri(&mesh, t);
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    let expected = 4000.0 * 100.0 * 2700.0 - 800.0 * 100.0 * 2100.0;
    assert!(((vol - expected) / expected).abs() < 1e-6, "volume {vol}");
}

#[test]
fn wall_footprint_axis_centered() {
    let (m, ids) = sample_room();
    let f = wall_footprint(&m, ids.walls[0]);
    let ys: Vec<f64> = f.iter().map(|p| p.y).collect();
    assert!(ys.contains(&-50.0) && ys.contains(&50.0));
}

#[test]
fn model_meshes_one_per_wall() {
    let (m, _) = sample_room();
    assert_eq!(model_meshes(&m).len(), 4);
}

#[test]
fn detect_one_room_sample() {
    let (m, ids) = sample_room();
    let cycles = detect_spaces(&m, ids.level, 1.0).unwrap();
    assert_eq!(cycles.len(), 1);
    let mut c = cycles[0].clone();
    assert_eq!(c.len(), 4);
    c.sort();
    let mut w = ids.walls.to_vec();
    w.sort();
    assert_eq!(c, w);
}

#[test]
fn detect_two_rooms_with_partition() {
    let (mut m, ids) = sample_room();
    // muri 0 e 2 spezzati non servono: il tramezzo collega i punti medi solo se
    // esistono nodi; usiamo un tramezzo da (2000,0) a (2000,3000) spezzando 0 e 2.
    let w0 = m.walls[&ids.walls[0]].clone();
    let w2 = m.walls[&ids.walls[2]].clone();
    m.openings.clear();
    m.walls.get_mut(&ids.walls[0]).unwrap().end = domain::Point2::new(2000.0, 0.0);
    let mut a = w0.clone();
    a.start = domain::Point2::new(2000.0, 0.0);
    let ia = m.alloc_id();
    m.walls.insert(ia, a);
    m.walls.get_mut(&ids.walls[2]).unwrap().end = domain::Point2::new(2000.0, 3000.0);
    let mut b = w2.clone();
    b.start = domain::Point2::new(2000.0, 3000.0);
    let ib = m.alloc_id();
    m.walls.insert(ib, b);
    let mut p = w0;
    p.start = domain::Point2::new(2000.0, 0.0);
    p.end = domain::Point2::new(2000.0, 3000.0);
    let ip = m.alloc_id();
    m.walls.insert(ip, p);
    let cycles = detect_spaces(&m, ids.level, 1.0).unwrap();
    assert_eq!(cycles.len(), 2);
    assert!(cycles.iter().all(|c| c.len() == 4 && c.contains(&ip)));
}

#[test]
fn open_contour_diagnosed() {
    let (mut m, ids) = sample_room();
    m.walls.remove(&ids.walls[2]);
    let e = detect_spaces(&m, ids.level, 1.0).unwrap_err();
    let SpaceDiag::OpenContour { mut dangling } = e;
    dangling.sort();
    assert_eq!(dangling, vec![ids.walls[1], ids.walls[3]]);
}

#[test]
fn net_area_sample() {
    let (m, ids) = sample_room();
    let a = space_area(&m, &ids.walls);
    assert!((a - 3900.0 * 2900.0).abs() < 1e-6, "area {a}");
}
