//! Rilevamento vani su grafo planare dei muri (G0).
//!
//! I nodi sono gli estremi d'asse dei muri del livello, fusi entro `tol_mm`;
//! gli archi sono i muri. Le facce sono ottenute con l'attraversamento delle
//! semi-arcate ordinate per angolo: le facce limitate risultano antiorarie
//! (area con segno > 0), la faccia esterna di ogni componente e' oraria ed e'
//! esclusa. G0 gestisce solo grafi planari semplici: incroci non nodati e muri
//! sovrapposti non sono rilevati.

use domain::{ElementId, Model, Point2};
use nalgebra::Vector2;
use petgraph::graph::{EdgeIndex, NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use std::collections::HashSet;
use thiserror::Error;

/// Diagnostica del rilevamento vani.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SpaceDiag {
    #[error("contorno aperto: muri con estremo libero {dangling:?}")]
    OpenContour { dangling: Vec<ElementId> },
    /// Configurazione fuori dal perimetro G0: incroci senza nodo comune,
    /// T-junction (estremo sulla parte interna di un altro muro), sovrapposizioni
    /// collineari, muri duplicati o con estremi fusi.
    #[error("geometria non supportata ({reason}): muri {walls:?}")]
    Unsupported {
        walls: Vec<ElementId>,
        reason: String,
    },
}

fn unsupported(mut walls: Vec<ElementId>, reason: &str) -> SpaceDiag {
    walls.sort();
    SpaceDiag::Unsupported {
        walls,
        reason: reason.to_owned(),
    }
}

fn dist_to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(&ab) / ab.norm_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).norm()
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    let (u, v) = (a - o, b - o);
    u.x * v.y - u.y * v.x
}

/// Verifica a coppie: estremo sulla parte interna di un altro muro
/// (T-junction o sovrapposizione collineare) e incroci propri.
fn check_supported(segs: &[(ElementId, Point2, Point2)], tol: f64) -> Result<(), SpaceDiag> {
    for (i, &(ia, a0, a1)) in segs.iter().enumerate() {
        for &(ib, b0, b1) in &segs[i + 1..] {
            let interior = |p: Point2, s: Point2, e: Point2| {
                (p - s).norm() > tol && (p - e).norm() > tol && dist_to_segment(p, s, e) <= tol
            };
            if interior(b0, a0, a1)
                || interior(b1, a0, a1)
                || interior(a0, b0, b1)
                || interior(a1, b0, b1)
            {
                return Err(unsupported(
                    vec![ia, ib],
                    "estremo sulla parte interna di un altro muro (T-junction o sovrapposizione)",
                ));
            }
            let (d1, d2) = (cross(a0, a1, b0), cross(a0, a1, b1));
            let (d3, d4) = (cross(b0, b1, a0), cross(b0, b1, a1));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                return Err(unsupported(vec![ia, ib], "incrocio senza nodo comune"));
            }
        }
    }
    Ok(())
}

fn node_for(g: &mut UnGraph<Point2, ElementId>, p: Point2, tol: f64) -> NodeIndex {
    g.node_indices()
        .find(|&i| (g[i] - p).norm() <= tol)
        .unwrap_or_else(|| g.add_node(p))
}

/// Cicli minimi (vani) sul livello, come liste di muri in ordine di contorno
/// (antiorario). `Err(OpenContour)` se un estremo ha grado 1.
pub fn detect_spaces(
    model: &Model,
    level: ElementId,
    tol_mm: f64,
) -> Result<Vec<Vec<ElementId>>, SpaceDiag> {
    let segs: Vec<(ElementId, Point2, Point2)> = model
        .walls
        .iter()
        .filter(|(_, w)| w.level == level)
        .map(|(&id, w)| (id, w.start, w.end))
        .collect();
    check_supported(&segs, tol_mm)?;
    let mut g: UnGraph<Point2, ElementId> = UnGraph::new_undirected();
    for &(id, s, e) in &segs {
        let a = node_for(&mut g, s, tol_mm);
        let b = node_for(&mut g, e, tol_mm);
        if a == b {
            return Err(unsupported(vec![id], "estremi fusi entro la tolleranza"));
        }
        if let Some(dup) = g.find_edge(a, b) {
            return Err(unsupported(vec![g[dup], id], "muri duplicati"));
        }
        g.add_edge(a, b, id);
    }
    let mut dangling: Vec<ElementId> = g
        .node_indices()
        .filter(|&n| g.edges(n).count() == 1)
        .flat_map(|n| g.edges(n).map(|e| *e.weight()).collect::<Vec<_>>())
        .collect();
    if !dangling.is_empty() {
        dangling.sort();
        dangling.dedup();
        return Err(SpaceDiag::OpenContour { dangling });
    }

    // Vicini di ogni nodo ordinati per angolo antiorario.
    let angle = |from: NodeIndex, to: NodeIndex| {
        let v = g[to] - g[from];
        v.y.atan2(v.x)
    };
    let around: Vec<Vec<(NodeIndex, EdgeIndex)>> = g
        .node_indices()
        .map(|n| {
            let mut v: Vec<_> = g
                .edges(n)
                .map(|e| (other(e.source(), e.target(), n), e.id()))
                .collect();
            v.sort_by(|a, b| angle(n, a.0).total_cmp(&angle(n, b.0)));
            v
        })
        .collect();

    let mut used: HashSet<(NodeIndex, EdgeIndex)> = HashSet::new();
    let mut faces = Vec::new();
    for e in g.edge_indices() {
        let (a, b) = g.edge_endpoints(e).expect("arco esistente");
        for start in [a, b] {
            if used.contains(&(start, e)) {
                continue;
            }
            // Semi-arco (start -> other via e).
            let mut walls = Vec::new();
            let mut pts: Vec<Point2> = Vec::new();
            let (mut u, mut edge) = (start, e);
            while used.insert((u, edge)) {
                let (x, y) = g.edge_endpoints(edge).expect("arco esistente");
                let v = other(x, y, u);
                walls.push(g[edge]);
                pts.push(g[u]);
                let ring = &around[v.index()];
                let k = ring
                    .iter()
                    .position(|&(nb, ed)| nb == u && ed == edge)
                    .expect("vicino presente");
                let (_, next) = ring[(k + ring.len() - 1) % ring.len()];
                u = v;
                edge = next;
            }
            if signed_area(&pts) > 0.0 {
                faces.push(walls);
            }
        }
    }
    Ok(faces)
}

fn other(x: NodeIndex, y: NodeIndex, n: NodeIndex) -> NodeIndex {
    if x == n { y } else { x }
}

fn signed_area(p: &[Point2]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Punto comune (in media) tra gli estremi piu' vicini di due muri.
fn joint(a: (Point2, Point2), b: (Point2, Point2)) -> Point2 {
    let mut best = (f64::INFINITY, a.0);
    for p in [a.0, a.1] {
        for q in [b.0, b.1] {
            let dd = (p - q).norm();
            if dd < best.0 {
                best = (dd, Point2::from((p.coords + q.coords) / 2.0));
            }
        }
    }
    best.1
}

/// Area netta (mm²) del vano: area di [`space_net_polygon`]. Restituisce 0 per
/// contorni non validi.
pub fn space_area(model: &Model, boundary: &[ElementId]) -> f64 {
    space_net_polygon(model, boundary).map_or(0.0, |p| signed_area(&p).abs())
}

/// Poligono netto del vano: estremi d'asse ridotti verso l'interno di meta'
/// spessore di ciascun muro di contorno, nello stesso verso del contorno.
/// Ignora `WallLocation` (usa sempre l'asse). `None` per contorni non validi.
pub fn space_net_polygon(model: &Model, boundary: &[ElementId]) -> Option<Vec<Point2>> {
    let walls: Vec<_> = boundary
        .iter()
        .map(|id| model.walls.get(id))
        .collect::<Option<_>>()?;
    let n = walls.len();
    if n < 3 {
        return None;
    }
    // vertice i = giunto tra muro i-1 e muro i; il muro i va da v[i] a v[i+1].
    let v: Vec<Point2> = (0..n)
        .map(|i| {
            let (a, b) = (walls[(i + n - 1) % n], walls[i]);
            joint((a.start, a.end), (b.start, b.end))
        })
        .collect();
    let sign = signed_area(&v).signum();
    if sign == 0.0 {
        return None;
    }
    // Rette d'asse traslate verso l'interno (sinistra se antiorario).
    let lines: Vec<(Point2, Vector2<f64>)> = (0..n)
        .map(|i| {
            let (a, b) = (v[i], v[(i + 1) % n]);
            let d = (b - a).normalize();
            let inward = Vector2::new(-d.y, d.x) * sign;
            (a + inward * (walls[i].thickness.as_mm() / 2.0), d)
        })
        .collect();
    let inset: Vec<Point2> = (0..n)
        .map(|i| {
            let (p1, d1) = lines[(i + n - 1) % n];
            let (p2, d2) = lines[i];
            let den = d1.x * d2.y - d1.y * d2.x;
            if den.abs() < 1e-12 {
                // Muri collineari: proietta il vertice sulla retta traslata.
                p2 + d2 * (v[i] - p2).dot(&d2)
            } else {
                let t = ((p2 - p1).x * d2.y - (p2 - p1).y * d2.x) / den;
                p1 + d1 * t
            }
        })
        .collect();
    Some(inset)
}
