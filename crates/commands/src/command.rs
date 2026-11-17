//! Tipi di comando: azioni, richieste, esiti ed errori.

use domain::{ElementId, ModelError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Numero di revisione del modello (0 = stato iniziale).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Revision(pub u64);

/// Identificatore di richiesta, usato per l'idempotenza dei retry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

/// Provenienza del comando.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Gui,
    Cli,
    Ai,
}

/// Azione atomica su modello. Gli input sono in millimetri.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
#[non_exhaustive]
pub enum Action {
    CreateWall {
        level: ElementId,
        start: [f64; 2],
        end: [f64; 2],
        thickness_mm: f64,
        height_mm: f64,
    },
    MoveWall {
        id: ElementId,
        dx_mm: f64,
        dy_mm: f64,
    },
    DeleteWall {
        id: ElementId,
    },
    AddDoor {
        host: ElementId,
        offset_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
    /// Sostituisce i vani del livello con quelli rilevati dal grafo dei muri.
    DetectSpaces {
        level: ElementId,
    },
    DeleteOpening {
        id: ElementId,
    },
}

impl Action {
    /// Nome breve per il journal.
    pub fn name(&self) -> &'static str {
        match self {
            Action::CreateWall { .. } => "CreateWall",
            Action::MoveWall { .. } => "MoveWall",
            Action::DeleteWall { .. } => "DeleteWall",
            Action::AddDoor { .. } => "AddDoor",
            Action::DeleteOpening { .. } => "DeleteOpening",
            Action::DetectSpaces { .. } => "DetectSpaces",
        }
    }
}

/// Transazione: tutte le azioni vengono applicate o nessuna.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub request_id: RequestId,
    pub expected: Revision,
    pub origin: Origin,
    pub actions: Vec<Action>,
    pub preview: bool,
}

/// Esito di un comando. Per una preview `revision` e' quella corrente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub revision: Revision,
    pub created: Vec<ElementId>,
    pub preview: bool,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum CommandError {
    #[error("conflitto di revisione: atteso {expected:?}, corrente {current:?}")]
    Conflict {
        expected: Revision,
        current: Revision,
    },
    #[error("modello non valido: {0}")]
    Invalid(#[from] ModelError),
    #[error("elemento {0:?} non trovato")]
    NotFound(ElementId),
    #[error("niente da annullare")]
    NothingToUndo,
    #[error("niente da ripristinare")]
    NothingToRedo,
    #[error("geometria: {0}")]
    Geometry(String),
    #[error("operazione non consentita")]
    Forbidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_serde_tag_op() {
        let a = Action::DeleteWall {
            id: ElementId::new(3),
        };
        let j = serde_json::to_string(&a).unwrap();
        assert!(j.contains("\"op\":\"DeleteWall\""));
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), a);
    }
}
