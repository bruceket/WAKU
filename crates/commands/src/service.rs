//! Servizio transazionale: clone del modello, applicazione, validazione, commit.

use crate::{Action, Command, CommandError, JournalEntry, Origin, Outcome, RequestId, Revision};
use domain::{
    ElementId, IfcGuid, Length, Model, Opening, OpeningKind, Point2, Space, Wall, WallLocation,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

/// Tolleranza di fusione degli estremi dei muri nel rilevamento vani (mm).
const SPACE_TOL_MM: f64 = 1.0;

/// Impronta deterministica (FNV-1a 64) di azioni serializzate + flag preview.
fn fingerprint(cmd: &Command) -> u64 {
    let json = serde_json::to_string(&cmd.actions).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in json.bytes().chain([u8::from(cmd.preview)]) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Vista immutabile del modello a una data revisione.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub revision: Revision,
    pub model: Arc<Model>,
}

/// Servizio comandi: unico punto di mutazione del modello.
pub struct CommandService {
    model: Arc<Model>,
    revision: Revision,
    undo: Vec<Arc<Model>>,
    redo: Vec<Arc<Model>>,
    journal: Vec<JournalEntry>,
    /// Esiti committati per `request_id`, con impronta delle azioni.
    /// Nota (R1): la mappa non e' limitata e cresce per tutta la sessione
    /// del servizio (una voce per transazione committata).
    seen: HashMap<RequestId, (u64, Outcome)>,
}

impl CommandService {
    pub fn new(model: Model) -> Self {
        CommandService {
            model: Arc::new(model),
            revision: Revision(0),
            undo: Vec::new(),
            redo: Vec::new(),
            journal: Vec::new(),
            seen: HashMap::new(),
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision,
            model: Arc::clone(&self.model),
        }
    }

    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    fn check_expected(&self, expected: Revision) -> Result<(), CommandError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(CommandError::Conflict {
                expected,
                current: self.revision,
            })
        }
    }

    fn commit(
        &mut self,
        next: Arc<Model>,
        request_id: RequestId,
        origin: Origin,
        summary: String,
    ) -> Revision {
        self.model = next;
        self.revision = Revision(self.revision.0 + 1);
        self.journal.push(JournalEntry {
            revision: self.revision,
            request_id,
            origin,
            summary,
            at: SystemTime::now(),
        });
        self.revision
    }

    /// Esegue una transazione. Un `request_id` gia' committato restituisce
    /// l'esito memorizzato senza nuova revisione; i fallimenti non sono memorizzati.
    /// Lo stesso `request_id` con azioni diverse produce [`CommandError::RequestIdReused`].
    pub fn execute(&mut self, cmd: Command) -> Result<Outcome, CommandError> {
        if !cmd.preview
            && let Some((fp, o)) = self.seen.get(&cmd.request_id)
        {
            if *fp != fingerprint(&cmd) {
                return Err(CommandError::RequestIdReused(cmd.request_id));
            }
            return Ok(o.clone());
        }
        self.check_expected(cmd.expected)?;
        let mut work = (*self.model).clone();
        let mut created = Vec::new();
        for a in &cmd.actions {
            apply(&mut work, a, &mut created)?;
        }
        work.validate()?;
        if cmd.preview {
            return Ok(Outcome {
                revision: self.revision,
                created,
                preview: true,
            });
        }
        let summary = cmd
            .actions
            .iter()
            .map(Action::name)
            .collect::<Vec<_>>()
            .join(", ");
        self.undo.push(Arc::clone(&self.model));
        self.redo.clear();
        let revision = self.commit(Arc::new(work), cmd.request_id.clone(), cmd.origin, summary);
        let outcome = Outcome {
            revision,
            created,
            preview: false,
        };
        let fp = fingerprint(&cmd);
        self.seen.insert(cmd.request_id, (fp, outcome.clone()));
        Ok(outcome)
    }

    /// Annulla l'ultima transazione; produce una nuova revisione.
    pub fn undo(&mut self, expected: Revision, origin: Origin) -> Result<Outcome, CommandError> {
        self.check_expected(expected)?;
        let prev = self.undo.pop().ok_or(CommandError::NothingToUndo)?;
        self.redo.push(Arc::clone(&self.model));
        let id = RequestId(format!("undo-{}", self.revision.0 + 1));
        let revision = self.commit(prev, id, origin, "Undo".to_owned());
        Ok(Outcome {
            revision,
            created: Vec::new(),
            preview: false,
        })
    }

    /// Ripristina l'ultima transazione annullata; produce una nuova revisione.
    pub fn redo(&mut self, expected: Revision, origin: Origin) -> Result<Outcome, CommandError> {
        self.check_expected(expected)?;
        let next = self.redo.pop().ok_or(CommandError::NothingToRedo)?;
        self.undo.push(Arc::clone(&self.model));
        let id = RequestId(format!("redo-{}", self.revision.0 + 1));
        let revision = self.commit(next, id, origin, "Redo".to_owned());
        Ok(Outcome {
            revision,
            created: Vec::new(),
            preview: false,
        })
    }
}

/// Applica una singola azione al modello di lavoro (la validazione globale
/// avviene a fine transazione).
fn apply(m: &mut Model, a: &Action, created: &mut Vec<ElementId>) -> Result<(), CommandError> {
    match a {
        Action::CreateWall {
            level,
            start,
            end,
            thickness_mm,
            height_mm,
        } => {
            if !m.levels.contains_key(level) {
                return Err(CommandError::NotFound(*level));
            }
            let id = m.alloc_id();
            m.walls.insert(
                id,
                Wall {
                    guid: IfcGuid::new(),
                    level: *level,
                    start: Point2::new(start[0], start[1]),
                    end: Point2::new(end[0], end[1]),
                    thickness: Length::mm(*thickness_mm),
                    height: Length::mm(*height_mm),
                    location: WallLocation::Axis,
                },
            );
            created.push(id);
        }
        Action::MoveWall { id, dx_mm, dy_mm } => {
            // Le aperture ospitate hanno offset relativo al muro: seguono da sole.
            let w = m.walls.get_mut(id).ok_or(CommandError::NotFound(*id))?;
            w.start.x += dx_mm;
            w.start.y += dy_mm;
            w.end.x += dx_mm;
            w.end.y += dy_mm;
            // I vani delimitati dal muro non sono piu' validi: rimossi in cascata
            // (vanno rilevati di nuovo con DetectSpaces).
            m.spaces.retain(|_, s| !s.boundary.contains(id));
        }
        Action::DeleteWall { id } => {
            m.walls.remove(id).ok_or(CommandError::NotFound(*id))?;
            m.openings.retain(|_, o| o.host != *id);
            m.spaces.retain(|_, s| !s.boundary.contains(id));
        }
        Action::AddDoor {
            host,
            offset_mm,
            width_mm,
            height_mm,
        } => {
            if !m.walls.contains_key(host) {
                return Err(CommandError::NotFound(*host));
            }
            let id = m.alloc_id();
            m.openings.insert(
                id,
                Opening {
                    guid: IfcGuid::new(),
                    host: *host,
                    kind: OpeningKind::Door,
                    offset: Length::mm(*offset_mm),
                    width: Length::mm(*width_mm),
                    height: Length::mm(*height_mm),
                    sill: Length::mm(0.0),
                },
            );
            created.push(id);
        }
        Action::DetectSpaces { level } => {
            if !m.levels.contains_key(level) {
                return Err(CommandError::NotFound(*level));
            }
            let cycles = geometry::detect_spaces(m, *level, SPACE_TOL_MM)
                .map_err(|e| CommandError::Geometry(e.to_string()))?;
            // Vani esistenti indicizzati per insieme di muri di contorno (ordine
            // ininfluente): un vano rilevato di nuovo mantiene id, GUID e nome.
            let key = |b: &[ElementId]| {
                let mut k = b.to_vec();
                k.sort();
                k
            };
            let mut old: HashMap<Vec<ElementId>, (ElementId, Space)> = HashMap::new();
            let ids: Vec<ElementId> = m
                .spaces
                .iter()
                .filter(|(_, s)| s.level == *level)
                .map(|(k, _)| *k)
                .collect();
            for k in ids {
                if let Some(s) = m.spaces.remove(&k) {
                    old.insert(key(&s.boundary), (k, s));
                }
            }
            for (i, boundary) in cycles.into_iter().enumerate() {
                let (id, guid, name) = match old.remove(&key(&boundary)) {
                    Some((k, s)) => (k, s.guid, s.name),
                    None => (m.alloc_id(), IfcGuid::new(), format!("Vano {}", i + 1)),
                };
                m.spaces.insert(
                    id,
                    Space {
                        guid,
                        level: *level,
                        boundary,
                        name,
                    },
                );
                created.push(id);
            }
        }
        Action::DeleteOpening { id } => {
            m.openings.remove(id).ok_or(CommandError::NotFound(*id))?;
        }
    }
    Ok(())
}

/// Richiesta al thread writer.
#[derive(Clone, Debug)]
pub enum Request {
    Execute(Command),
    Undo {
        expected: Revision,
        origin: Origin,
    },
    Redo {
        expected: Revision,
        origin: Origin,
    },
    Snapshot,
    /// Copia del journal delle revisioni committate (con origine).
    Journal,
}

/// Risposta del thread writer.
#[derive(Clone, Debug)]
pub enum Response {
    Done(Result<Outcome, CommandError>),
    Snapshot(Snapshot),
    Journal(Vec<JournalEntry>),
}

enum Msg {
    Call(Request, Sender<Response>),
    Subscribe(Sender<Snapshot>),
}

/// Handle clonabile verso il thread writer. Il thread termina quando tutti gli
/// handle sono stati rilasciati.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Sender<Msg>,
}

impl WriterHandle {
    /// Invia una richiesta; la risposta arriva sul `Receiver` restituito.
    pub fn send(&self, req: Request) -> Receiver<Response> {
        let (rtx, rrx) = mpsc::channel();
        // Se il writer e' terminato il Receiver risulta disconnesso: l'errore
        // e' visibile al chiamante su `recv()`.
        let _ = self.tx.send(Msg::Call(req, rtx));
        rrx
    }

    /// Riceve uno Snapshot dopo ogni revisione committata.
    pub fn subscribe(&self) -> Receiver<Snapshot> {
        let (stx, srx) = mpsc::channel();
        let _ = self.tx.send(Msg::Subscribe(stx));
        srx
    }
}

/// Avvia il thread writer dedicato che possiede il servizio.
pub fn spawn_writer(mut svc: CommandService) -> WriterHandle {
    let (tx, rx) = mpsc::channel::<Msg>();
    thread::spawn(move || {
        let mut subs: Vec<Sender<Snapshot>> = Vec::new();
        for msg in rx {
            match msg {
                Msg::Subscribe(s) => subs.push(s),
                Msg::Call(req, reply) => {
                    let before = svc.revision();
                    let resp = match req {
                        Request::Execute(c) => Response::Done(svc.execute(c)),
                        Request::Undo { expected, origin } => {
                            Response::Done(svc.undo(expected, origin))
                        }
                        Request::Redo { expected, origin } => {
                            Response::Done(svc.redo(expected, origin))
                        }
                        Request::Snapshot => Response::Snapshot(svc.snapshot()),
                        Request::Journal => Response::Journal(svc.journal().to_vec()),
                    };
                    if svc.revision() != before {
                        let snap = svc.snapshot();
                        subs.retain(|s| s.send(snap.clone()).is_ok());
                    }
                    let _ = reply.send(resp);
                }
            }
        }
    });
    WriterHandle { tx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::fixtures::sample_room;
    use std::thread;

    fn cmd(id: &str, expected: u64, actions: Vec<Action>) -> Command {
        Command {
            request_id: RequestId(id.to_owned()),
            expected: Revision(expected),
            origin: Origin::Gui,
            actions,
            preview: false,
        }
    }

    fn wall_action(level: domain::ElementId, x: f64) -> Action {
        Action::CreateWall {
            level,
            start: [x, 0.0],
            end: [x, 1000.0],
            thickness_mm: 100.0,
            height_mm: 2700.0,
        }
    }

    #[test]
    fn ac02_move_wall_with_door_and_undo() {
        let (m, ids) = sample_room();
        let original = m.clone();
        let mut svc = CommandService::new(m);
        let out = svc
            .execute(cmd(
                "r1",
                0,
                vec![Action::MoveWall {
                    id: ids.walls[0],
                    dx_mm: 0.0,
                    dy_mm: 500.0,
                }],
            ))
            .unwrap();
        assert_eq!(out.revision, Revision(1));
        let snap = svc.snapshot();
        assert_eq!(snap.model.walls[&ids.walls[0]].start.y, 500.0);
        assert_eq!(snap.model.openings[&ids.door].offset.as_mm(), 500.0);
        assert_eq!(snap.model.validate(), Ok(()));
        svc.undo(Revision(1), Origin::Gui).unwrap();
        assert_eq!(*svc.snapshot().model, original);
        assert_eq!(svc.revision(), Revision(2));
        svc.redo(Revision(2), Origin::Gui).unwrap();
        assert_eq!(svc.revision(), Revision(3));
        assert_eq!(svc.snapshot().model.walls[&ids.walls[0]].start.y, 500.0);
        assert_eq!(svc.journal().len(), 3);
    }

    #[test]
    fn stale_revision_conflict() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(cmd("a", 0, vec![wall_action(ids.level, 10.0)]))
            .unwrap();
        let e = svc
            .execute(cmd("b", 0, vec![wall_action(ids.level, 20.0)]))
            .unwrap_err();
        assert_eq!(
            e,
            CommandError::Conflict {
                expected: Revision(0),
                current: Revision(1)
            }
        );
        assert_eq!(
            svc.undo(Revision(0), Origin::Cli).unwrap_err(),
            CommandError::Conflict {
                expected: Revision(0),
                current: Revision(1)
            }
        );
    }

    #[test]
    fn ac15_retry_same_request_id_no_duplicate() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let c = cmd("same", 0, vec![wall_action(ids.level, 10.0)]);
        let o1 = svc.execute(c.clone()).unwrap();
        let o2 = svc.execute(c).unwrap();
        assert_eq!(o1, o2);
        assert_eq!(svc.snapshot().model.walls.len(), 5);
        assert_eq!(svc.revision(), Revision(1));
    }

    #[test]
    fn reused_request_id_with_different_actions_rejected() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(cmd("same", 0, vec![wall_action(ids.level, 10.0)]))
            .unwrap();
        let e = svc
            .execute(cmd("same", 1, vec![wall_action(ids.level, 20.0)]))
            .unwrap_err();
        assert_eq!(e, CommandError::RequestIdReused(RequestId("same".into())));
        assert_eq!(svc.revision(), Revision(1));
        assert_eq!(svc.snapshot().model.walls.len(), 5);
    }

    #[test]
    fn detect_spaces_twice_keeps_space_guid() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let det = |id: &str, rev| cmd(id, rev, vec![Action::DetectSpaces { level: ids.level }]);
        svc.execute(det("d1", 0)).unwrap();
        let first: Vec<_> = svc
            .snapshot()
            .model
            .spaces
            .iter()
            .map(|(k, s)| (*k, s.guid.clone()))
            .collect();
        assert_eq!(first.len(), 1);
        svc.execute(det("d2", 1)).unwrap();
        let second: Vec<_> = svc
            .snapshot()
            .model
            .spaces
            .iter()
            .map(|(k, s)| (*k, s.guid.clone()))
            .collect();
        assert_eq!(first, second);
        assert_eq!(svc.snapshot().model.validate(), Ok(()));
    }

    #[test]
    fn invalid_batch_rolls_back() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m.clone());
        let e = svc
            .execute(cmd(
                "x",
                0,
                vec![
                    wall_action(ids.level, 10.0),
                    Action::AddDoor {
                        host: ids.walls[0],
                        offset_mm: 0.0,
                        width_mm: 99999.0,
                        height_mm: 2100.0,
                    },
                ],
            ))
            .unwrap_err();
        assert!(matches!(e, CommandError::Invalid(_)));
        assert_eq!(*svc.snapshot().model, m);
        assert_eq!(svc.revision(), Revision(0));
        assert!(svc.journal().is_empty());
    }

    #[test]
    fn failed_request_not_cached() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let bad = Action::DeleteWall {
            id: domain::ElementId::new(9999),
        };
        assert_eq!(
            svc.execute(cmd("r", 0, vec![bad])).unwrap_err(),
            CommandError::NotFound(domain::ElementId::new(9999))
        );
        assert!(
            svc.execute(cmd("r", 0, vec![wall_action(ids.level, 10.0)]))
                .is_ok()
        );
    }

    #[test]
    fn preview_does_not_commit() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m.clone());
        let mut c = cmd("p", 0, vec![wall_action(ids.level, 10.0)]);
        c.preview = true;
        let o = svc.execute(c).unwrap();
        assert!(o.preview);
        assert_eq!(o.created.len(), 1);
        assert_eq!(o.revision, Revision(0));
        assert_eq!(svc.revision(), Revision(0));
        assert_eq!(*svc.snapshot().model, m);
        assert!(svc.journal().is_empty());
    }

    #[test]
    fn door_past_wall_end_rejected() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m.clone());
        let e = svc
            .execute(cmd(
                "d",
                0,
                vec![Action::AddDoor {
                    host: ids.walls[0],
                    offset_mm: 3800.0,
                    width_mm: 800.0,
                    height_mm: 2100.0,
                }],
            ))
            .unwrap_err();
        assert!(matches!(
            e,
            CommandError::Invalid(domain::ModelError::OpeningOutOfHost(_))
        ));
        assert_eq!(*svc.snapshot().model, m);
    }

    #[test]
    fn zero_length_wall_rejected() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let e = svc
            .execute(cmd(
                "z",
                0,
                vec![Action::CreateWall {
                    level: ids.level,
                    start: [5.0, 5.0],
                    end: [5.0, 5.0],
                    thickness_mm: 100.0,
                    height_mm: 2700.0,
                }],
            ))
            .unwrap_err();
        assert!(matches!(
            e,
            CommandError::Invalid(domain::ModelError::ZeroLengthWall(_))
        ));
    }

    #[test]
    fn delete_wall_removes_hosted_openings() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(cmd("d", 0, vec![Action::DeleteWall { id: ids.walls[0] }]))
            .unwrap();
        assert!(svc.snapshot().model.openings.is_empty());
    }

    #[test]
    fn nan_input_rejected() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let e = svc
            .execute(cmd(
                "n",
                0,
                vec![Action::MoveWall {
                    id: ids.walls[1],
                    dx_mm: f64::NAN,
                    dy_mm: 0.0,
                }],
            ))
            .unwrap_err();
        assert!(matches!(
            e,
            CommandError::Invalid(domain::ModelError::NonFinite { .. })
        ));
    }

    #[test]
    fn redo_cleared_on_new_execute() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(cmd("a", 0, vec![wall_action(ids.level, 10.0)]))
            .unwrap();
        svc.undo(Revision(1), Origin::Gui).unwrap();
        svc.execute(cmd("b", 2, vec![wall_action(ids.level, 20.0)]))
            .unwrap();
        assert_eq!(
            svc.redo(Revision(3), Origin::Gui).unwrap_err(),
            CommandError::NothingToRedo
        );
    }

    #[test]
    fn detect_spaces_action_creates_space() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        let a = Action::DetectSpaces { level: ids.level };
        let o = svc.execute(cmd("s1", 0, vec![a.clone()])).unwrap();
        assert_eq!(o.created.len(), 1);
        let snap = svc.snapshot();
        let s = &snap.model.spaces[&o.created[0]];
        assert_eq!(s.name, "Vano 1");
        assert_eq!(s.level, ids.level);
        assert_eq!(s.boundary.len(), 4);
        // Rieseguire sostituisce, non duplica.
        let o2 = svc.execute(cmd("s2", 1, vec![a])).unwrap();
        let snap = svc.snapshot();
        assert_eq!(snap.model.spaces.len(), 1);
        assert!(snap.model.spaces.contains_key(&o2.created[0]));
    }

    #[test]
    fn detect_spaces_open_contour_leaves_model() {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m.clone());
        let e = svc
            .execute(cmd(
                "o",
                0,
                vec![
                    Action::DeleteWall { id: ids.walls[2] },
                    Action::DetectSpaces { level: ids.level },
                ],
            ))
            .unwrap_err();
        assert!(matches!(e, CommandError::Geometry(_)));
        assert_eq!(*svc.snapshot().model, m);
        assert_eq!(svc.revision(), Revision(0));
    }

    fn with_space() -> (CommandService, domain::fixtures::SampleIds) {
        let (m, ids) = sample_room();
        let mut svc = CommandService::new(m);
        svc.execute(cmd(
            "sp",
            0,
            vec![Action::DetectSpaces { level: ids.level }],
        ))
        .unwrap();
        assert_eq!(svc.snapshot().model.spaces.len(), 1);
        (svc, ids)
    }

    #[test]
    fn delete_wall_removes_bounding_spaces() {
        let (mut svc, ids) = with_space();
        svc.execute(cmd("d", 1, vec![Action::DeleteWall { id: ids.walls[1] }]))
            .unwrap();
        assert!(svc.snapshot().model.spaces.is_empty());
    }

    #[test]
    fn move_wall_removes_bounding_spaces() {
        let (mut svc, ids) = with_space();
        svc.execute(cmd(
            "m",
            1,
            vec![Action::MoveWall {
                id: ids.walls[1],
                dx_mm: 100.0,
                dy_mm: 0.0,
            }],
        ))
        .unwrap();
        assert!(svc.snapshot().model.spaces.is_empty());
    }

    #[test]
    fn detect_spaces_unsupported_is_geometry_error() {
        let (mut m, ids) = sample_room();
        let mut w = m.walls[&ids.walls[1]].clone();
        std::mem::swap(&mut w.start, &mut w.end);
        let id = m.alloc_id();
        m.walls.insert(id, w);
        let mut svc = CommandService::new(m.clone());
        let e = svc
            .execute(cmd("u", 0, vec![Action::DetectSpaces { level: ids.level }]))
            .unwrap_err();
        assert!(matches!(e, CommandError::Geometry(_)));
        assert_eq!(*svc.snapshot().model, m);
    }

    #[test]
    fn writer_thread_serializes() {
        let (m, ids) = sample_room();
        let h = spawn_writer(CommandService::new(m));
        let sub = h.subscribe();
        let level = ids.level;
        let workers: Vec<_> = (0..8)
            .map(|t| {
                let h = h.clone();
                thread::spawn(move || {
                    for i in 0..100 {
                        let id = format!("t{t}-{i}");
                        loop {
                            let Response::Snapshot(s) = h.send(Request::Snapshot).recv().unwrap()
                            else {
                                panic!("risposta inattesa")
                            };
                            let c = cmd(&id, s.revision.0, vec![wall_action(level, i as f64)]);
                            match h.send(Request::Execute(c)).recv().unwrap() {
                                Response::Done(Ok(_)) => break,
                                Response::Done(Err(CommandError::Conflict { .. })) => continue,
                                other => panic!("risposta inattesa: {other:?}"),
                            }
                        }
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        let Response::Snapshot(s) = h.send(Request::Snapshot).recv().unwrap() else {
            panic!("risposta inattesa")
        };
        assert_eq!(s.model.walls.len(), 804);
        assert_eq!(s.revision, Revision(800));
        for n in 1..=800u64 {
            assert_eq!(sub.recv().unwrap().revision, Revision(n));
        }
    }

    #[test]
    fn writer_journal_request_reports_origins() {
        let (m, ids) = sample_room();
        let h = spawn_writer(CommandService::new(m));
        let mut c = cmd("j1", 0, vec![Action::DeleteOpening { id: ids.door }]);
        c.origin = Origin::Ai;
        h.send(Request::Execute(c)).recv().unwrap();
        h.send(Request::Undo {
            expected: Revision(1),
            origin: Origin::Gui,
        })
        .recv()
        .unwrap();
        match h.send(Request::Journal).recv().unwrap() {
            Response::Journal(j) => {
                assert_eq!(j.len(), 2);
                assert_eq!(j[0].origin, Origin::Ai);
                assert_eq!(j[1].origin, Origin::Gui);
            }
            other => panic!("risposta inattesa: {other:?}"),
        }
    }
}
