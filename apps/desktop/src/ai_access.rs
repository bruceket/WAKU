//! Bridge MCP (named pipe) su runtime tokio dedicato e pannello "Accesso AI".

use commands::{JournalEntry, Origin, WriterHandle};
use mcp::{CancellationToken, Policy, Token};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Stato del server sulla pipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipeState {
    Active,
    Stopped(String),
}

/// Una "generazione" del server: ogni writer (progetto) ha policy, stato e
/// segnale di arresto propri. Un task vecchio non puo' toccare la generazione nuova.
pub struct Generation {
    pub policy: Arc<Policy>,
    state: Arc<Mutex<PipeState>>,
    shutdown: CancellationToken,
}

impl Generation {
    fn new(initial: PipeState) -> Self {
        Self {
            policy: Arc::new(Policy::new()),
            state: Arc::new(Mutex::new(initial)),
            shutdown: CancellationToken::new(),
        }
    }

    fn set_state(&self, s: PipeState) {
        set(&self.state, s);
    }

    pub fn state(&self) -> PipeState {
        self.state
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| PipeState::Stopped("stato non leggibile".to_owned()))
    }
}

fn set(state: &Mutex<PipeState>, s: PipeState) {
    if let Ok(mut g) = state.lock() {
        *g = s;
    }
}

pub struct AiBridge {
    runtime: Option<tokio::runtime::Runtime>,
    token: Option<Token>,
    token_path: Option<PathBuf>,
    pipe: Option<String>,
    current: Generation,
}

impl AiBridge {
    fn inert() -> Self {
        Self {
            runtime: None,
            token: None,
            token_path: None,
            pipe: None,
            current: Generation::new(PipeState::Stopped("non avviato".to_owned())),
        }
    }

    /// Genera il token, scrive il file token e avvia il server sulla pipe utente.
    pub fn start(handle: WriterHandle) -> Self {
        let mut b = Self::inert();
        let setup = (|| -> Result<(), String> {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("waku-mcp-bridge")
                .enable_all()
                .build()
                .map_err(|e| format!("runtime: {e}"))?;
            let token = Token::generate().map_err(|e| format!("token: {e}"))?;
            let dir = mcp::token::default_token_dir().ok_or("LOCALAPPDATA non definita")?;
            let path = mcp::token::write_token_file(&dir, &token)
                .map_err(|e| format!("file token: {e}"))?;
            let pipe = mcp::pipe_name().map_err(|e| format!("nome pipe: {e}"))?;
            b.runtime = Some(rt);
            b.token = Some(token);
            b.token_path = Some(path);
            b.pipe = Some(pipe);
            Ok(())
        })();
        match setup {
            Ok(()) => b.serve(handle),
            Err(e) => b.current.set_state(PipeState::Stopped(e)),
        }
        b
    }

    /// Chiude la generazione corrente (accept e connessioni aperte) e ne crea una
    /// nuova con policy negata.
    fn next_generation(&mut self) {
        self.current.shutdown.cancel();
        self.current.policy.revoke();
        self.current = Generation::new(PipeState::Stopped("non avviato".to_owned()));
    }

    fn serve(&mut self, handle: WriterHandle) {
        let (Some(rt), Some(token), Some(pipe)) = (&self.runtime, &self.token, &self.pipe) else {
            return;
        };
        let g = &self.current;
        let (policy, token, pipe) = (Arc::clone(&g.policy), token.clone(), pipe.clone());
        let (shutdown, state) = (g.shutdown.clone(), Arc::clone(&g.state));
        g.set_state(PipeState::Active);
        rt.spawn(async move {
            let r = mcp::serve_on(&pipe, handle, policy, token, shutdown).await;
            let msg = match r {
                Ok(()) => "arrestato".to_owned(),
                Err(e) => e.to_string(),
            };
            set(&state, PipeState::Stopped(msg));
        });
    }

    /// Nuovo writer (nuovo progetto / apertura): nuova generazione sullo stesso
    /// nome pipe. La scrittura AI torna negata.
    pub fn restart(&mut self, handle: WriterHandle) {
        self.next_generation();
        self.serve(handle);
    }

    pub fn state(&self) -> PipeState {
        self.current.state()
    }

    /// Arresta il server, attende il runtime e rimuove il file token.
    pub fn shutdown(&mut self) {
        self.current.shutdown.cancel();
        self.current.policy.revoke();
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_timeout(std::time::Duration::from_secs(2));
        }
        if let Some(p) = self.token_path.take() {
            let _ = std::fs::remove_file(p);
        }
    }

    /// Pannello "Accesso AI".
    pub fn ui(&self, ui: &mut egui::Ui, journal: &[JournalEntry]) {
        ui.heading("Accesso AI");
        let state = self.state();
        egui::Grid::new("ai_state").num_columns(2).show(ui, |ui| {
            ui.label("Pipe:");
            ui.label(self.pipe.as_deref().unwrap_or("non disponibile"));
            ui.end_row();
            ui.label("Stato:");
            match &state {
                PipeState::Active => ui.label("in ascolto"),
                PipeState::Stopped(m) => ui.label(format!("non attivo ({m})")),
            };
            ui.end_row();
            ui.label("Scrittura AI:");
            ui.label(if self.current.policy.allows_write() {
                "concessa per questa sessione"
            } else {
                "negata (sola lettura)"
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let granted = self.current.policy.allows_write();
            if ui
                .add_enabled(
                    !granted,
                    egui::Button::new("Concedi scrittura per questa sessione"),
                )
                .clicked()
            {
                self.current.policy.grant();
            }
            if ui
                .add_enabled(granted, egui::Button::new("Revoca"))
                .clicked()
            {
                self.current.policy.revoke();
            }
        });
        ui.separator();
        ui.label("Journal");
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("journal")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Rev.");
                        ui.strong("Origine");
                        ui.strong("Operazione");
                        ui.end_row();
                        for e in journal.iter().rev() {
                            ui.label(e.revision.0.to_string());
                            ui.label(origin_label(e.origin));
                            ui.label(&e.summary);
                            ui.end_row();
                        }
                    });
            });
    }
}

pub fn origin_label(o: Origin) -> &'static str {
    match o {
        Origin::Gui => "GUI",
        Origin::Cli => "CLI",
        Origin::Ai => "AI",
    }
}

impl Drop for AiBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_yields_fresh_revoked_policy() {
        let mut b = AiBridge::inert();
        b.current.set_state(PipeState::Active);
        b.current.policy.grant();
        let (old_policy, old_state) = (Arc::clone(&b.current.policy), Arc::clone(&b.current.state));
        let old_stop = b.current.shutdown.clone();
        b.next_generation();
        assert!(old_stop.is_cancelled());
        assert!(!old_policy.allows_write());
        assert!(!Arc::ptr_eq(&old_policy, &b.current.policy));
        assert!(!b.current.policy.allows_write());
        // Concedere sulla nuova generazione non riabilita la vecchia.
        b.current.policy.grant();
        assert!(!old_policy.allows_write());
        // Il task vecchio che termina non sovrascrive lo stato nuovo.
        b.current.set_state(PipeState::Active);
        set(&old_state, PipeState::Stopped("arrestato".to_owned()));
        assert_eq!(b.state(), PipeState::Active);
    }
}
