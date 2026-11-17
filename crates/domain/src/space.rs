//! Spazio (stanza) delimitato da muri.

use crate::{ElementId, IfcGuid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub guid: IfcGuid,
    pub level: ElementId,
    /// Muri di contorno, in ordine.
    pub boundary: Vec<ElementId>,
    pub name: String,
}
