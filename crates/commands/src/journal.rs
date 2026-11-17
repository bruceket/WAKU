//! Journal delle revisioni committate.

use crate::{Origin, RequestId, Revision};
use std::time::SystemTime;

/// Voce di journal: una per ogni revisione committata (incluse undo/redo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub revision: Revision,
    pub request_id: RequestId,
    pub origin: Origin,
    pub summary: String,
    pub at: SystemTime,
}
