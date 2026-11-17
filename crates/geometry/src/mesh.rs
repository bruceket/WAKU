//! Mesh triangolare indicizzata.

use domain::ElementId;

/// Mesh di un elemento: posizioni f64 in mm (mondo, Z su), normali per vertice.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub element: ElementId,
}
