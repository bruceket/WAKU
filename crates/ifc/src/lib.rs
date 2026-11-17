//! Export IFC4 (ADD2 TC1) in formato STEP (ISO 10303-21) per il sottoinsieme G0.
//!
//! Scritto a mano a partire dallo schema ufficiale buildingSMART; output
//! byte-deterministico a parita' di modello e di timestamp dell'header.

mod export;
pub mod step;

pub use export::*;
