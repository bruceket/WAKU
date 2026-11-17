//! Geometria derivata (G0): mesh dei muri con aperture e rilevamento vani.
//!
//! Dipende solo da `domain`. Coordinate in millimetri, mondo con Z verso l'alto.

pub mod mesh;
pub mod spaces;
pub mod wall_mesh;

pub use mesh::*;
pub use spaces::*;
pub use wall_mesh::*;
