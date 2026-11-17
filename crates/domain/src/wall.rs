//! Muro: segmento 2D in pianta con spessore e altezza.

use crate::{ElementId, IfcGuid, Length};
use serde::{Deserialize, Serialize};

/// Punto 2D in pianta, in millimetri.
pub type Point2 = nalgebra::Point2<f64>;

/// Posizione della linea di definizione rispetto allo spessore del muro.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallLocation {
    Interior,
    Axis,
    Exterior,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub guid: IfcGuid,
    pub level: ElementId,
    pub start: Point2,
    pub end: Point2,
    pub thickness: Length,
    pub height: Length,
    pub location: WallLocation,
}

impl Wall {
    /// Lunghezza dell'asse (mm).
    pub fn length(&self) -> Length {
        Length::mm((self.end - self.start).norm())
    }
}
