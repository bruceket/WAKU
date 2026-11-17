//! Logica testabile dell'app desktop (indipendente da egui).

use commands::{
    Action, Command, JournalEntry, Origin, Request, RequestId, Response, Revision, Snapshot,
    WriterHandle,
};
use domain::{ElementId, Length, LengthUnit, parse_length};
use std::sync::mpsc::{Receiver, TryRecvError};

/// Variante del logo da disegnare.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogoVariant {
    FullLight,
    FullDark,
    SymbolLight,
    SymbolDark,
}

/// Sotto questa altezza si disegna solo il simbolo (AC-24).
pub const LOGO_SYMBOL_THRESHOLD_PX: f32 = 24.0;

/// Sceglie la variante del logo per tema e altezza (px logici).
pub fn logo_variant(dark: bool, height_px: f32) -> LogoVariant {
    let symbol_only = height_px < LOGO_SYMBOL_THRESHOLD_PX;
    match (dark, symbol_only) {
        (false, false) => LogoVariant::FullLight,
        (true, false) => LogoVariant::FullDark,
        (false, true) => LogoVariant::SymbolLight,
        (true, true) => LogoVariant::SymbolDark,
    }
}

/// Valuta il campo lunghezza (espressione, unita' di progetto di default).
/// Accetta solo lunghezze finite e positive.
pub fn length_field_commit(text: &str, unit: LengthUnit) -> Result<Length, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("inserire una lunghezza".to_owned());
    }
    let l = parse_length(t, unit).map_err(|e| format!("espressione non valida: {e}"))?;
    if !l.as_mm().is_finite() || l.as_mm() <= 0.0 {
        return Err("la lunghezza deve essere positiva".to_owned());
    }
    Ok(l)
}

/// Messaggio della barra di stato.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusMsg {
    pub text: String,
    pub error: bool,
}

/// Stato GUI del modello: ogni mutazione passa dal [`CommandSink`] con origine `Gui`.
pub struct Controller<S: CommandSink> {
    sink: S,
    snapshot: Snapshot,
    updates: Receiver<Snapshot>,
    pending: Vec<Receiver<Response>>,
    journal_rx: Option<Receiver<Response>>,
    journal: Vec<JournalEntry>,
    status: Option<StatusMsg>,
    /// Revisione attesa per la prossima richiesta: max(snapshot, ultimo esito).
    expected: Revision,
    seq: u64,
}

impl<S: CommandSink> Controller<S> {
    /// Si iscrive agli snapshot e legge lo stato iniziale.
    pub fn new(sink: S) -> Result<Self, String> {
        let updates = sink.subscribe();
        let snapshot = match sink.send(Request::Snapshot).recv() {
            Ok(Response::Snapshot(s)) => s,
            _ => return Err("writer non disponibile".to_owned()),
        };
        let expected = snapshot.revision;
        let journal_rx = Some(sink.send(Request::Journal));
        Ok(Self {
            sink,
            snapshot,
            updates,
            pending: Vec::new(),
            journal_rx,
            journal: Vec::new(),
            status: None,
            expected,
            seq: 0,
        })
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    pub fn status(&self) -> Option<&StatusMsg> {
        self.status.as_ref()
    }

    pub fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(StatusMsg {
            text: text.into(),
            error,
        });
    }

    /// Invia una transazione con `expected` = revisione corrente.
    pub fn execute(&mut self, actions: Vec<Action>) {
        self.seq += 1;
        let cmd = Command {
            request_id: RequestId(format!("gui-{}-{}", std::process::id(), self.seq)),
            expected: self.expected,
            origin: Origin::Gui,
            actions,
            preview: false,
        };
        self.pending.push(self.sink.send(Request::Execute(cmd)));
    }

    pub fn create_wall(
        &mut self,
        level: ElementId,
        start: [f64; 2],
        end: [f64; 2],
        thickness_mm: f64,
        height_mm: f64,
    ) {
        self.execute(vec![Action::CreateWall {
            level,
            start,
            end,
            thickness_mm,
            height_mm,
        }]);
    }

    pub fn add_door(&mut self, host: ElementId, offset_mm: f64, width_mm: f64, height_mm: f64) {
        self.execute(vec![Action::AddDoor {
            host,
            offset_mm,
            width_mm,
            height_mm,
        }]);
    }

    pub fn delete_wall(&mut self, id: ElementId) {
        self.execute(vec![Action::DeleteWall { id }]);
    }

    pub fn detect_spaces(&mut self, level: ElementId) {
        self.execute(vec![Action::DetectSpaces { level }]);
    }

    pub fn undo(&mut self) {
        let req = Request::Undo {
            expected: self.expected,
            origin: Origin::Gui,
        };
        self.pending.push(self.sink.send(req));
    }

    pub fn redo(&mut self) {
        let req = Request::Redo {
            expected: self.expected,
            origin: Origin::Gui,
        };
        self.pending.push(self.sink.send(req));
    }

    /// Raccoglie risposte e snapshot senza bloccare. `true` se il modello e' cambiato.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(s) = self.updates.try_recv() {
            if s.revision > self.snapshot.revision {
                self.snapshot = s;
                self.expected = self.expected.max(self.snapshot.revision);
                changed = true;
            }
        }
        let mut still = Vec::new();
        for rx in self.pending.drain(..) {
            match rx.try_recv() {
                Ok(Response::Done(Ok(o))) => {
                    if !o.preview {
                        self.expected = self.expected.max(o.revision);
                    }
                    self.status = Some(StatusMsg {
                        text: format!("Revisione {}", o.revision.0),
                        error: false,
                    });
                }
                Ok(Response::Done(Err(e))) => {
                    self.status = Some(StatusMsg {
                        text: e.to_string(),
                        error: true,
                    });
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => still.push(rx),
                Err(TryRecvError::Disconnected) => {
                    self.status = Some(StatusMsg {
                        text: "writer non disponibile".to_owned(),
                        error: true,
                    });
                }
            }
        }
        self.pending = still;
        if changed {
            self.journal_rx = Some(self.sink.send(Request::Journal));
        }
        if let Some(rx) = &self.journal_rx {
            match rx.try_recv() {
                Ok(Response::Journal(j)) => {
                    self.journal = j;
                    self.journal_rx = None;
                }
                Err(TryRecvError::Empty) => {}
                _ => self.journal_rx = None,
            }
        }
        changed
    }

    /// `true` se ci sono richieste in attesa di risposta.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || self.journal_rx.is_some()
    }
}

/// Destinatario delle richieste di comando (il thread writer o un finto nei test).
pub trait CommandSink {
    fn send(&self, req: Request) -> Receiver<Response>;
    fn subscribe(&self) -> Receiver<Snapshot>;
}

impl CommandSink for WriterHandle {
    fn send(&self, req: Request) -> Receiver<Response> {
        WriterHandle::send(self, req)
    }
    fn subscribe(&self) -> Receiver<Snapshot> {
        WriterHandle::subscribe(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commands::{Action, CommandError, Origin, Outcome, Revision};
    use domain::fixtures::sample_room;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::mpsc;

    #[test]
    fn logo_variant_cases() {
        assert_eq!(logo_variant(true, 32.0), LogoVariant::FullDark);
        assert_eq!(logo_variant(false, 16.0), LogoVariant::SymbolLight);
        assert_eq!(logo_variant(false, 24.0), LogoVariant::FullLight);
        assert_eq!(logo_variant(true, 23.9), LogoVariant::SymbolDark);
    }

    #[test]
    fn length_field_ac01() {
        let l = length_field_commit("332-60", LengthUnit::Cm).unwrap();
        assert_eq!(l.to_unit(LengthUnit::Cm), 272.0);
        assert_eq!(
            length_field_commit(" 2.5 m ", LengthUnit::Cm)
                .unwrap()
                .as_mm(),
            2500.0
        );
        assert!(length_field_commit("", LengthUnit::Cm).is_err());
        assert!(length_field_commit("10-20", LengthUnit::Cm).is_err());
        assert!(length_field_commit("3+", LengthUnit::Cm).is_err());
    }

    /// Finto writer: registra le richieste e risponde subito.
    struct FakeSink {
        log: Rc<RefCell<Vec<Request>>>,
        snapshot: Snapshot,
        fail: Option<CommandError>,
    }

    impl CommandSink for FakeSink {
        fn send(&self, req: Request) -> Receiver<Response> {
            self.log.borrow_mut().push(req.clone());
            let (tx, rx) = mpsc::channel();
            let resp = match req {
                Request::Snapshot => Response::Snapshot(self.snapshot.clone()),
                Request::Journal => Response::Journal(Vec::new()),
                _ => Response::Done(match &self.fail {
                    Some(e) => Err(e.clone()),
                    None => Ok(Outcome {
                        revision: Revision(self.snapshot.revision.0 + 1),
                        created: Vec::new(),
                        preview: false,
                    }),
                }),
            };
            tx.send(resp).unwrap();
            rx
        }
        fn subscribe(&self) -> Receiver<Snapshot> {
            mpsc::channel().1
        }
    }

    fn fake(fail: Option<CommandError>) -> (FakeSink, Rc<RefCell<Vec<Request>>>) {
        let (m, _) = sample_room();
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = FakeSink {
            log: Rc::clone(&log),
            snapshot: Snapshot {
                revision: Revision(7),
                model: Arc::new(m),
            },
            fail,
        };
        (sink, log)
    }

    #[test]
    fn gui_actions_go_through_writer() {
        let (m, ids) = sample_room();
        let (sink, log) = fake(None);
        let mut c = Controller::new(sink).unwrap();
        c.create_wall(ids.level, [0.0, 0.0], [1000.0, 0.0], 100.0, 2700.0);
        c.add_door(ids.walls[0], 100.0, 800.0, 2100.0);
        c.delete_wall(ids.walls[1]);
        c.detect_spaces(ids.level);
        c.undo();
        c.redo();
        c.poll();
        let log = log.borrow();
        let mutations: Vec<&Request> = log
            .iter()
            .filter(|r| !matches!(r, Request::Snapshot | Request::Journal))
            .collect();
        assert_eq!(mutations.len(), 6);
        for r in &mutations {
            match r {
                Request::Execute(cmd) => {
                    assert_eq!(cmd.origin, Origin::Gui);
                    assert_eq!(cmd.expected, Revision(7));
                    assert!(!cmd.preview);
                }
                Request::Undo { expected, origin } | Request::Redo { expected, origin } => {
                    assert_eq!(*origin, Origin::Gui);
                    assert_eq!(*expected, Revision(7));
                }
                other => panic!("richiesta inattesa: {other:?}"),
            }
        }
        let Request::Execute(first) = mutations[0] else {
            panic!("attesa Execute")
        };
        assert!(matches!(
            first.actions[0],
            Action::CreateWall { thickness_mm, height_mm, .. }
                if thickness_mm == 100.0 && height_mm == 2700.0
        ));
        // Request id univoci.
        let ids: std::collections::HashSet<_> = mutations
            .iter()
            .filter_map(|r| match r {
                Request::Execute(c) => Some(c.request_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 4);
        // Il modello non e' mai toccato direttamente dalla GUI.
        assert_eq!(c.snapshot().model.walls.len(), m.walls.len());
        assert_eq!(c.snapshot().revision, Revision(7));
    }

    #[test]
    fn undo_right_after_commit_uses_new_revision() {
        let (_, ids) = sample_room();
        let (sink, log) = fake(None);
        let mut c = Controller::new(sink).unwrap();
        c.create_wall(ids.level, [0.0, 0.0], [1000.0, 0.0], 100.0, 2700.0);
        // L'esito (rev. 8) arriva prima dello snapshot dalla sottoscrizione.
        c.poll();
        c.undo();
        let log = log.borrow();
        match log.last().unwrap() {
            Request::Undo { expected, .. } => assert_eq!(*expected, Revision(8)),
            other => panic!("richiesta inattesa: {other:?}"),
        }
    }

    #[test]
    fn command_error_shown_in_status() {
        let (sink, _) = fake(Some(CommandError::NothingToUndo));
        let mut c = Controller::new(sink).unwrap();
        c.undo();
        c.poll();
        let s = c.status().unwrap();
        assert!(s.error);
        assert_eq!(s.text, "niente da annullare");
    }
}
