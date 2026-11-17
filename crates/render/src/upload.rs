//! Logica CPU dell'upload incrementale: hash di contenuto, piano di upload, origine locale.
use domain::ElementId;
use geometry::Mesh;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Soglia oltre la quale il centro bbox richiede un nuovo origine (100 m in mm).
pub const REORIGIN_THRESHOLD_MM: f64 = 1.0e5;

/// Hash a 64 bit del contenuto geometrico (posizioni, normali, indici).
pub fn content_hash(mesh: &Mesh) -> u64 {
    let mut h = DefaultHasher::new();
    for p in &mesh.positions {
        for c in p {
            c.to_bits().hash(&mut h);
        }
    }
    for n in &mesh.normals {
        for c in n {
            c.to_bits().hash(&mut h);
        }
    }
    mesh.indices.hash(&mut h);
    h.finish()
}

/// Statistiche di un upload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadStats {
    pub reused: usize,
    pub uploaded: usize,
    pub removed: usize,
}

/// Piano di upload: quali elementi riusare, caricare, rimuovere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadPlan {
    pub reuse: Vec<ElementId>,
    pub upload: Vec<ElementId>,
    pub remove: Vec<ElementId>,
}

impl UploadPlan {
    pub fn stats(&self) -> UploadStats {
        UploadStats {
            reused: self.reuse.len(),
            uploaded: self.upload.len(),
            removed: self.remove.len(),
        }
    }
}

/// Confronta lo stato GPU corrente (id → hash) con le nuove mesh.
pub fn plan_upload(current: &HashMap<ElementId, u64>, new: &[(ElementId, u64)]) -> UploadPlan {
    let mut plan = UploadPlan::default();
    let mut seen = HashSet::with_capacity(new.len());
    for &(id, hash) in new {
        seen.insert(id);
        if current.get(&id) == Some(&hash) {
            plan.reuse.push(id);
        } else {
            plan.upload.push(id);
        }
    }
    let mut removed: Vec<ElementId> = current
        .keys()
        .filter(|id| !seen.contains(id))
        .copied()
        .collect();
    removed.sort();
    plan.remove = removed;
    plan
}

/// Decide l'origine: fissata al primo upload non vuoto, cambiata solo se il nuovo centro
/// bbox si allontana più di [`REORIGIN_THRESHOLD_MM`]. Ritorna `(origine, re-origin)`.
pub fn next_origin(current: Option<[f64; 3]>, center: Option<[f64; 3]>) -> ([f64; 3], bool) {
    match (current, center) {
        (Some(o), None) => (o, false),
        (None, None) => ([0.0; 3], false),
        (None, Some(c)) => (c, true),
        (Some(o), Some(c)) => {
            let d = ((c[0] - o[0]).powi(2) + (c[1] - o[1]).powi(2) + (c[2] - o[2]).powi(2)).sqrt();
            if d > REORIGIN_THRESHOLD_MM {
                (c, true)
            } else {
                (o, false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(v: u64) -> ElementId {
        ElementId::new(v)
    }

    #[test]
    fn unchanged_meshes_reused() {
        let current: HashMap<_, _> = (1..=5).map(|i| (id(i), i * 10)).collect();
        let new: Vec<_> = (1..=5)
            .map(|i| (id(i), if i == 3 { 999 } else { i * 10 }))
            .collect();
        let plan = plan_upload(&current, &new);
        assert_eq!(
            plan.stats(),
            UploadStats {
                reused: 4,
                uploaded: 1,
                removed: 0
            }
        );
        assert_eq!(plan.upload, vec![id(3)]);

        let fewer: Vec<_> = (1..=4).map(|i| (id(i), i * 10)).collect();
        assert_eq!(plan_upload(&current, &fewer).remove, vec![id(5)]);
    }

    #[test]
    fn small_origin_drift_no_full_reupload() {
        let o = [1.0e7, 1.0e7, 0.0];
        assert_eq!(next_origin(None, Some(o)), (o, true));
        // spostamento di 5 m: nessun re-origin
        assert_eq!(
            next_origin(Some(o), Some([1.0e7 + 5000.0, 1.0e7, 0.0])),
            (o, false)
        );
        // oltre 100 m: re-origin
        let far = [1.0e7 + 2.0e5, 1.0e7, 0.0];
        assert_eq!(next_origin(Some(o), Some(far)), (far, true));
        // scena vuota: origine invariata
        assert_eq!(next_origin(Some(o), None), (o, false));
    }

    #[test]
    fn content_hash_detects_move() {
        let m = Mesh {
            positions: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]],
            indices: vec![0],
            element: id(1),
        };
        let mut moved = m.clone();
        moved.positions[0][0] = 1.0;
        assert_eq!(content_hash(&m), content_hash(&m.clone()));
        assert_ne!(content_hash(&m), content_hash(&moved));
    }
}
