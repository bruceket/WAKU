//! Apertura (porta) ospitata in un muro.

use crate::{ElementId, IfcGuid, Length};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub guid: IfcGuid,
    pub host: ElementId,
    pub kind: OpeningKind,
    /// Distanza lungo l'asse dallo start del muro al bordo sinistro.
    pub offset: Length,
    pub width: Length,
    pub height: Length,
    pub sill: Length,
}
