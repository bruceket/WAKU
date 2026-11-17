//! `waku-cli`: interfaccia a riga di comando headless per i progetti `.waku`.
//!
//! Codici di uscita: 0 ok, 1 input non valido / I/O / file corrotto,
//! 2 progetto bloccato ("progetto aperto altrove").

use clap::{Parser, Subcommand};
use commands::{
    Action, Command, CommandService, Origin, RequestId, Revision, Snapshot, model_summary,
};
use domain::{LengthUnit, Model};
use ifc::{ExportOptions, export_ifc4_with};
use persistence::{PersistError, ProjectLock};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "waku-cli", version, about = "CLI headless di Waku")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Crea un progetto vuoto
    New {
        file: PathBuf,
        /// Unita' di lunghezza: mm, cm, m
        #[arg(long, default_value = "mm")]
        unit: String,
    },
    /// Crea il progetto di esempio (stanza 4x3 m con porta e vano)
    Sample { file: PathBuf },
    /// Applica un file JSON di azioni (Vec<Action>) come singola transazione
    Apply { file: PathBuf, actions: PathBuf },
    /// Esporta in IFC4
    ExportIfc { file: PathBuf, out: PathBuf },
    /// Stampa il riepilogo JSON del modello
    Info { file: PathBuf },
}

enum Fail {
    Input(String),
    Locked,
}

impl From<PersistError> for Fail {
    fn from(e: PersistError) -> Self {
        match e {
            PersistError::Locked => Fail::Locked,
            other => Fail::Input(other.to_string()),
        }
    }
}

fn io<E: std::fmt::Display>(e: E) -> Fail {
    Fail::Input(e.to_string())
}

fn command(expected: Revision, actions: Vec<Action>) -> Command {
    Command {
        request_id: RequestId(uuid::Uuid::new_v4().to_string()),
        expected,
        origin: Origin::Cli,
        actions,
        preview: false,
    }
}

fn run(cli: Cli) -> Result<(), Fail> {
    match cli.cmd {
        Cmd::New { file, unit } => {
            let u = LengthUnit::from_suffix(&unit)
                .ok_or_else(|| Fail::Input(format!("unita' non valida: {unit} (mm, cm, m)")))?;
            let name = file.file_stem().map_or_else(
                || "Progetto".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            );
            save_locked(&file, &Model::new(&name, u))
        }
        Cmd::Sample { file } => {
            let mut svc = CommandService::new(Model::new("Stanza di esempio", LengthUnit::Mm));
            let level = *svc
                .snapshot()
                .model
                .levels
                .keys()
                .next()
                .ok_or_else(|| Fail::Input("livello mancante".into()))?;
            let wall = |s: [f64; 2], e: [f64; 2]| Action::CreateWall {
                level,
                start: s,
                end: e,
                thickness_mm: 100.0,
                height_mm: 2700.0,
            };
            let out = svc
                .execute(command(
                    svc.revision(),
                    vec![
                        wall([0.0, 0.0], [4000.0, 0.0]),
                        wall([4000.0, 0.0], [4000.0, 3000.0]),
                        wall([4000.0, 3000.0], [0.0, 3000.0]),
                        wall([0.0, 3000.0], [0.0, 0.0]),
                    ],
                ))
                .map_err(io)?;
            svc.execute(command(
                svc.revision(),
                vec![
                    Action::AddDoor {
                        host: out.created[0],
                        offset_mm: 500.0,
                        width_mm: 800.0,
                        height_mm: 2100.0,
                    },
                    Action::DetectSpaces { level },
                ],
            ))
            .map_err(io)?;
            save_locked(&file, &svc.snapshot().model)
        }
        Cmd::Apply { file, actions } => {
            let _lock = ProjectLock::acquire(&file)?;
            let model = persistence::load(&file)?;
            let text = std::fs::read_to_string(&actions).map_err(io)?;
            let acts: Vec<Action> = serde_json::from_str(&text)
                .map_err(|e| Fail::Input(format!("JSON azioni non valido: {e}")))?;
            let mut svc = CommandService::new(model);
            svc.execute(command(svc.revision(), acts)).map_err(io)?;
            persistence::save(&file, &svc.snapshot().model)?;
            Ok(())
        }
        Cmd::ExportIfc { file, out } => {
            let model = persistence::load(&file)?;
            let mut opts = ExportOptions::default();
            if let Ok(ts) = std::env::var("WAKU_FIXED_TIMESTAMP") {
                opts.timestamp = ts;
            }
            if let Some(n) = out.file_name() {
                opts.file_name = n.to_string_lossy().into_owned();
            }
            let mut skipped = Vec::new();
            persistence::write_atomic_with(&out, |mut w| {
                let rep = export_ifc4_with(&model, &mut w, &opts).map_err(std::io::Error::other)?;
                skipped = rep.skipped;
                Ok(())
            })?;
            for (id, why) in &skipped {
                eprintln!("saltato elemento {}: {why}", id.get());
            }
            Ok(())
        }
        Cmd::Info { file } => {
            let model = persistence::load(&file)?;
            let svc = CommandService::new(model);
            let snap: Snapshot = svc.snapshot();
            println!(
                "{}",
                serde_json::to_string_pretty(&model_summary(&snap)).map_err(io)?
            );
            Ok(())
        }
    }
}

fn save_locked(file: &Path, model: &Model) -> Result<(), Fail> {
    let _lock = ProjectLock::acquire(file)?;
    persistence::save(file, model)?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail::Locked) => {
            eprintln!("errore: progetto aperto altrove (lock presente)");
            ExitCode::from(2)
        }
        Err(Fail::Input(m)) => {
            eprintln!("errore: {m}");
            ExitCode::from(1)
        }
    }
}
