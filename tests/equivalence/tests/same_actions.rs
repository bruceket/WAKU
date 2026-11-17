//! Equivalenza tra canali (AC-24): le stesse azioni applicate direttamente al
//! `CommandService` (Gui), via `waku-cli apply` e via bridge IPC + client
//! producono lo stesso modello (normalizzato) e lo stesso IFC (normalizzato).
//!
//! Sequenza: T1 = 4 muri (rettangolo 4000x3000, sp. 100, h 2700) + porta +
//! DetectSpaces; T2 = MoveWall del primo muro (dy 500; rimuove il vano che il
//! muro delimita, e un nuovo DetectSpaces fallirebbe per T-junction senza
//! pulizia giunzioni); poi Undo e Redo.
//!
//! La CLI non ha un comando di undo/redo: per il canale CLI si applicano T1 e
//! T2 senza Undo/Redo, mentre (a) e (c) eseguono Undo poi Redo (effetto netto
//! nullo). I tre modelli finali devono comunque coincidere.
//!
//! Gli `IfcGuid` di dominio sono casuali per canale: il confronto del modello
//! scarta guid e id e confronta la struttura; l'IFC viene confrontato dopo
//! avere sostituito ogni GUID di 22 caratteri con un segnaposto numerato in
//! ordine di prima comparsa, con timestamp fisso.

use commands::{Action, Command, CommandService, Origin, RequestId, Revision};
use domain::{ElementId, LengthUnit, Model};
use ifc::{ExportOptions, export_ifc4_with};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

const TS: &str = "2026-01-01T00:00:00";
const NAME: &str = "equiv";
const ALPHABET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let p = std::env::temp_dir().join(format!("waku-equiv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn empty_model() -> Model {
    Model::new(NAME, LengthUnit::Mm)
}

fn wall(level: ElementId, s: [f64; 2], e: [f64; 2]) -> Action {
    Action::CreateWall {
        level,
        start: s,
        end: e,
        thickness_mm: 100.0,
        height_mm: 2700.0,
    }
}

fn cmd(expected: Revision, actions: Vec<Action>) -> Command {
    Command {
        request_id: RequestId(format!("equiv-{}", expected.0)),
        expected,
        origin: Origin::Gui,
        actions,
        preview: false,
    }
}

/// (T1, T2, livello) con gli id calcolati su un servizio di prova: gli id sono
/// deterministici a partire dal modello vuoto.
fn transactions() -> (Vec<Action>, Vec<Action>, ElementId) {
    let mut probe = CommandService::new(empty_model());
    let level = *probe.snapshot().model.levels.keys().next().unwrap();
    let walls = vec![
        wall(level, [0.0, 0.0], [4000.0, 0.0]),
        wall(level, [4000.0, 0.0], [4000.0, 3000.0]),
        wall(level, [4000.0, 3000.0], [0.0, 3000.0]),
        wall(level, [0.0, 3000.0], [0.0, 0.0]),
    ];
    let out = probe.execute(cmd(Revision(0), walls.clone())).unwrap();
    let first = out.created[0];
    let mut t1 = walls;
    t1.push(Action::AddDoor {
        host: first,
        offset_mm: 500.0,
        width_mm: 800.0,
        height_mm: 2100.0,
    });
    t1.push(Action::DetectSpaces { level });
    // Lo spostamento di dy 500 porta gli estremi del muro sulla parte interna
    // dei muri adiacenti (T-junction): G0 non fa pulizia delle giunzioni e
    // DetectSpaces rifiuterebbe la geometria. Quindi nessun DetectSpaces dopo
    // il MoveWall: il vano che il muro delimitava viene rimosso.
    let t2 = vec![Action::MoveWall {
        id: first,
        dx_mm: 0.0,
        dy_mm: 500.0,
    }];
    (t1, t2, level)
}

/// Forma normalizzata: niente guid, niente id; riferimenti per indice.
fn normalize_model(m: &Model) -> String {
    let wall_ids: Vec<ElementId> = m.walls.keys().copied().collect();
    let idx = |id: &ElementId| wall_ids.iter().position(|w| w == id).unwrap();
    let mut s = String::new();
    writeln!(s, "unit={:?}", m.project.unit).unwrap();
    for w in m.walls.values() {
        writeln!(
            s,
            "wall start=({:?},{:?}) end=({:?},{:?}) t={:?} h={:?} loc={:?}",
            w.start.x,
            w.start.y,
            w.end.x,
            w.end.y,
            w.thickness.as_mm(),
            w.height.as_mm(),
            w.location
        )
        .unwrap();
    }
    for o in m.openings.values() {
        writeln!(
            s,
            "opening host={} kind={:?} off={:?} w={:?} h={:?} sill={:?}",
            idx(&o.host),
            o.kind,
            o.offset.as_mm(),
            o.width.as_mm(),
            o.height.as_mm(),
            o.sill.as_mm()
        )
        .unwrap();
    }
    for sp in m.spaces.values() {
        let b: Vec<usize> = sp.boundary.iter().map(idx).collect();
        writeln!(s, "space {:?} boundary={b:?}", sp.name).unwrap();
    }
    s
}

/// Sostituisce ogni GUID IFC (stringa tra apici di 22 caratteri dell'alfabeto
/// IFC) con un segnaposto numerato per prima comparsa.
fn normalize_ifc(text: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut out = String::with_capacity(text.len());
    for (i, seg) in text.split('\'').enumerate() {
        if i > 0 {
            out.push('\'');
        }
        if i % 2 == 1 && seg.len() == 22 && seg.chars().all(|c| ALPHABET.contains(c)) {
            let n = seen.iter().position(|g| g == seg).unwrap_or_else(|| {
                seen.push(seg.to_owned());
                seen.len() - 1
            });
            write!(out, "GUID#{n}").unwrap();
        } else {
            out.push_str(seg);
        }
    }
    out
}

fn entity_count(text: &str) -> usize {
    text.lines().filter(|l| l.starts_with('#')).count()
}

fn export(m: &Model) -> String {
    let mut buf = Vec::new();
    let opts = ExportOptions {
        timestamp: TS.to_owned(),
        file_name: format!("{NAME}.ifc"),
    };
    export_ifc4_with(m, &mut buf, &opts).unwrap();
    String::from_utf8(buf).unwrap()
}

// ---- canale (a): CommandService diretto, origin Gui ----

fn channel_direct(t1: &[Action], t2: &[Action]) -> Model {
    let mut svc = CommandService::new(empty_model());
    svc.execute(cmd(svc.revision(), t1.to_vec())).unwrap();
    svc.execute(cmd(svc.revision(), t2.to_vec())).unwrap();
    svc.undo(svc.revision(), Origin::Gui).unwrap();
    svc.redo(svc.revision(), Origin::Gui).unwrap();
    (*svc.snapshot().model).clone()
}

// ---- canale (b): waku-cli apply (senza Undo/Redo, assenti nella CLI) ----

fn cli_exe() -> PathBuf {
    // Il binario sta nella cartella del profilo (target/debug), antenata
    // dell'eseguibile di test; il layout interno di cargo puo' variare.
    let name = format!("waku-cli{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().unwrap();
    let find = || exe.ancestors().map(|d| d.join(&name)).find(|p| p.is_file());
    if let Some(p) = find() {
        return p;
    }
    let st = Proc::new(env!("CARGO"))
        .args(["build", "-p", "cli"])
        .status()
        .unwrap();
    assert!(st.success(), "build di waku-cli fallita");
    find().expect("waku-cli non trovato")
}

fn run_cli(args: &[&Path], env_ts: bool) {
    let mut c = Proc::new(cli_exe());
    c.args(args);
    if env_ts {
        c.env("WAKU_FIXED_TIMESTAMP", TS);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "waku-cli {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn channel_cli(t1: &[Action], t2: &[Action], level: ElementId) -> (Model, String) {
    let d = Tmp::new("cli");
    let file = d.0.join(format!("{NAME}.waku"));
    run_cli(&[Path::new("new"), &file], false);
    let fresh = persistence::load(&file).unwrap();
    assert_eq!(fresh.levels.keys().next(), Some(&level), "id livello");
    for (n, t) in [("t1", t1), ("t2", t2)] {
        let j = d.0.join(format!("{n}.json"));
        std::fs::write(&j, serde_json::to_string(t).unwrap()).unwrap();
        run_cli(&[Path::new("apply"), &file, &j], false);
    }
    let ifc = d.0.join(format!("{NAME}.ifc"));
    run_cli(&[Path::new("export-ifc"), &file, &ifc], true);
    (
        persistence::load(&file).unwrap(),
        std::fs::read_to_string(&ifc).unwrap(),
    )
}

// ---- canale (c): bridge IPC + BridgeClient (solo Windows) ----

#[cfg(windows)]
fn channel_bridge(t1: &[Action], t2: &[Action]) -> Model {
    use commands::{Request, Response, spawn_writer};
    use mcp::{
        BridgeClient, BridgeRequest, BridgeResponse, CancellationToken, Policy, ServeConfig, Token,
        pipe_name_with_suffix, serve_with, token::write_token_file,
    };
    use std::sync::Arc;

    let dir = Tmp::new("bridge");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let handle = spawn_writer(CommandService::new(empty_model()));
        let policy = Arc::new(Policy::new());
        policy.grant();
        let token = Token::generate().unwrap();
        write_token_file(&dir.0, &token).unwrap();
        let name = pipe_name_with_suffix(&format!("equiv-{}", std::process::id())).unwrap();
        let stop = CancellationToken::new();
        let (n, h, p, s) = (name.clone(), handle.clone(), policy, stop.clone());
        tokio::spawn(async move {
            serve_with(&n, ServeConfig::default(), h, p, token, s)
                .await
                .unwrap()
        });
        let mut client = None;
        for _ in 0..300 {
            if let Ok(c) = BridgeClient::connect(&name, &dir.0).await {
                client = Some(c);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let mut c = client.expect("bridge non raggiungibile");
        let mut rev = 0u64;
        for (id, t) in [("equiv-t1", t1), ("equiv-t2", t2)] {
            let r = c
                .request(BridgeRequest::Envelope {
                    request_id: id.to_owned(),
                    expected_revision: rev,
                    actions: t.to_vec(),
                    preview: false,
                })
                .await
                .unwrap();
            let BridgeResponse::Ok { revision, .. } = r else {
                panic!("risposta inattesa: {r:?}")
            };
            rev = revision;
        }
        for req in [
            BridgeRequest::Undo {
                expected_revision: rev,
            },
            BridgeRequest::Redo {
                expected_revision: rev + 1,
            },
        ] {
            let r = c.request(req).await.unwrap();
            assert!(matches!(r, BridgeResponse::Ok { .. }), "{r:?}");
        }
        let Response::Snapshot(snap) = handle.send(Request::Snapshot).recv().unwrap() else {
            panic!("snapshot atteso")
        };
        stop.cancel();
        (*snap.model).clone()
    })
}

#[test]
fn same_actions_same_model() {
    let (t1, t2, level) = transactions();
    let a = channel_direct(&t1, &t2);
    let (b, b_ifc) = channel_cli(&t1, &t2, level);

    // sanita' del risultato: 4 muri, porta, vano rimosso dal MoveWall
    assert_eq!(a.walls.len(), 4);
    assert_eq!(a.openings.len(), 1);
    assert_eq!(a.spaces.len(), 0);

    let na = normalize_model(&a);
    assert_eq!(na, normalize_model(&b), "modello CLI diverso da diretto");

    let ia = normalize_ifc(&export(&a));
    let ib = normalize_ifc(&b_ifc);
    assert_eq!(entity_count(&ia), entity_count(&ib));
    assert_eq!(ia, ib, "IFC CLI diverso da diretto");

    #[cfg(windows)]
    {
        let c = channel_bridge(&t1, &t2);
        assert_eq!(na, normalize_model(&c), "modello bridge diverso da diretto");
        let ic = normalize_ifc(&export(&c));
        assert_eq!(entity_count(&ia), entity_count(&ic));
        assert_eq!(ia, ic, "IFC bridge diverso da diretto");
    }
}
