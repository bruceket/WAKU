//! Mesh dei muri: prisma con fori rettangolari per le aperture ospitate.
//!
//! Convenzioni:
//! - posizioni in mm nel sistema mondo, Z verso l'alto, base del muro alla quota
//!   del livello;
//! - `WallLocation::Axis`: linea di definizione al centro dello spessore;
//!   `Exterior`: la linea e' la faccia esterna, il corpo si estende a SINISTRA
//!   della direzione start->end (in un contorno antiorario: verso l'interno);
//!   `Interior`: la linea e' la faccia interna, il corpo si estende a DESTRA.
//! - Giunzioni: nessuna pulizia in G0; i muri che si incontrano possono
//!   sovrapporsi negli angoli (overlap ammesso).

use crate::Mesh;
use domain::{ElementId, Model, Point2, Wall, WallLocation};
use nalgebra::Vector2;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GeomError {
    #[error("muro {0:?} non trovato")]
    WallNotFound(ElementId),
    #[error("livello {0:?} non trovato")]
    LevelNotFound(ElementId),
    #[error("muro {0:?} degenere")]
    Degenerate(ElementId),
}

/// Intervallo laterale [s0, s1] del corpo del muro lungo la normale sinistra.
fn lateral(w: &Wall) -> (f64, f64) {
    let t = w.thickness.as_mm();
    match w.location {
        WallLocation::Axis => (-t / 2.0, t / 2.0),
        WallLocation::Exterior => (0.0, t),
        WallLocation::Interior => (-t, 0.0),
    }
}

/// Direzione unitaria e normale sinistra del muro.
fn frame(w: &Wall) -> Option<(Vector2<f64>, Vector2<f64>)> {
    let d = w.end - w.start;
    let len = d.norm();
    if len.is_nan() || len <= 0.0 {
        return None;
    }
    let d = d / len;
    Some((d, Vector2::new(-d.y, d.x)))
}

/// Impronta in pianta del muro: start-destra, end-destra, end-sinistra,
/// start-sinistra (antioraria). Per un muro inesistente o degenere restituisce
/// quattro punti nell'origine.
pub fn wall_footprint(model: &Model, id: ElementId) -> [Point2; 4] {
    let Some(w) = model.walls.get(&id) else {
        return [Point2::origin(); 4];
    };
    let Some((_, n)) = frame(w) else {
        return [w.start; 4];
    };
    let (s0, s1) = lateral(w);
    [
        w.start + n * s0,
        w.end + n * s0,
        w.end + n * s1,
        w.start + n * s1,
    ]
}

struct Builder {
    mesh: Mesh,
}

impl Builder {
    /// Aggiunge un quad planare con normale uscente `nrm`, correggendo
    /// l'avvolgimento se necessario.
    fn quad(&mut self, q: [[f64; 3]; 4], nrm: [f64; 3]) {
        let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let (u, v) = (sub(q[1], q[0]), sub(q[2], q[0]));
        let c = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let flip = c[0] * nrm[0] + c[1] * nrm[1] + c[2] * nrm[2] < 0.0;
        let base = self.mesh.positions.len() as u32;
        let nf = nrm.map(|x| x as f32);
        for p in q {
            self.mesh.positions.push(p);
            self.mesh.normals.push(nf);
        }
        let order: [u32; 6] = if flip {
            [0, 2, 1, 0, 3, 2]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        self.mesh.indices.extend(order.iter().map(|k| base + k));
    }
}

/// Mesh chiusa del muro con un foro passante per ogni apertura ospitata
/// (facce interne del foro incluse). L'elevazione (u lungo l'asse, z) e'
/// suddivisa in una griglia sui bordi delle aperture: celle piene producono
/// facce fronte/retro, i bordi pieno/vuoto producono le facce laterali.
pub fn wall_mesh(model: &Model, id: ElementId) -> Result<Mesh, GeomError> {
    let w = model.walls.get(&id).ok_or(GeomError::WallNotFound(id))?;
    let level = model
        .levels
        .get(&w.level)
        .ok_or(GeomError::LevelNotFound(w.level))?;
    let (d, n) = frame(w).ok_or(GeomError::Degenerate(id))?;
    let len = w.length().as_mm();
    let h = w.height.as_mm();
    let z0 = level.elevation.as_mm();
    let (s0, s1) = lateral(w);

    let holes: Vec<(f64, f64, f64, f64)> = model
        .openings
        .values()
        .filter(|o| o.host == id)
        .map(|o| {
            let u0 = o.offset.as_mm().clamp(0.0, len);
            let u1 = (o.offset.as_mm() + o.width.as_mm()).clamp(0.0, len);
            let v0 = o.sill.as_mm().clamp(0.0, h);
            let v1 = (o.sill.as_mm() + o.height.as_mm()).clamp(0.0, h);
            (u0, u1, v0, v1)
        })
        .filter(|(u0, u1, v0, v1)| u1 > u0 && v1 > v0)
        .collect();

    let mut us = vec![0.0, len];
    let mut vs = vec![0.0, h];
    for &(u0, u1, v0, v1) in &holes {
        us.extend([u0, u1]);
        vs.extend([v0, v1]);
    }
    for a in [&mut us, &mut vs] {
        a.sort_by(f64::total_cmp);
        a.dedup();
    }
    let (nu, nv) = (us.len() - 1, vs.len() - 1);
    let solid = |i: isize, j: isize| -> bool {
        if i < 0 || j < 0 || i as usize >= nu || j as usize >= nv {
            return false;
        }
        let (i, j) = (i as usize, j as usize);
        let (cu, cv) = ((us[i] + us[i + 1]) / 2.0, (vs[j] + vs[j + 1]) / 2.0);
        !holes
            .iter()
            .any(|&(u0, u1, v0, v1)| cu > u0 && cu < u1 && cv > v0 && cv < v1)
    };
    let p = |u: f64, s: f64, z: f64| -> [f64; 3] {
        let q = w.start + d * u + n * s;
        [q.x, q.y, z0 + z]
    };
    let n3 = [n.x, n.y, 0.0];
    let d3 = [d.x, d.y, 0.0];
    let neg = |a: [f64; 3]| a.map(|x| -x);

    let mut b = Builder {
        mesh: Mesh {
            positions: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            element: id,
        },
    };
    for i in 0..nu {
        for j in 0..nv {
            let (ii, jj) = (i as isize, j as isize);
            if !solid(ii, jj) {
                continue;
            }
            let (u0, u1, v0, v1) = (us[i], us[i + 1], vs[j], vs[j + 1]);
            // Facce laterali (sinistra s1, destra s0).
            b.quad(
                [p(u0, s1, v0), p(u1, s1, v0), p(u1, s1, v1), p(u0, s1, v1)],
                n3,
            );
            b.quad(
                [p(u0, s0, v0), p(u1, s0, v0), p(u1, s0, v1), p(u0, s0, v1)],
                neg(n3),
            );
            // Bordi verso celle vuote o esterne.
            if !solid(ii - 1, jj) {
                b.quad(
                    [p(u0, s0, v0), p(u0, s1, v0), p(u0, s1, v1), p(u0, s0, v1)],
                    neg(d3),
                );
            }
            if !solid(ii + 1, jj) {
                b.quad(
                    [p(u1, s0, v0), p(u1, s1, v0), p(u1, s1, v1), p(u1, s0, v1)],
                    d3,
                );
            }
            if !solid(ii, jj - 1) {
                b.quad(
                    [p(u0, s0, v0), p(u1, s0, v0), p(u1, s1, v0), p(u0, s1, v0)],
                    [0.0, 0.0, -1.0],
                );
            }
            if !solid(ii, jj + 1) {
                b.quad(
                    [p(u0, s0, v1), p(u1, s0, v1), p(u1, s1, v1), p(u0, s1, v1)],
                    [0.0, 0.0, 1.0],
                );
            }
        }
    }
    Ok(b.mesh)
}

/// Mesh di tutti i muri del modello (i muri non validi sono saltati).
pub fn model_meshes(model: &Model) -> Vec<Mesh> {
    model
        .walls
        .keys()
        .filter_map(|&id| wall_mesh(model, id).ok())
        .collect()
}
