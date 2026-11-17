//! Applicazione eframe: pannelli, menu, scorciatoie e lavoro di I/O su thread worker.

use crate::ai_access::AiBridge;
use crate::app::Controller;
use crate::plan_view::{PlanView, door_offset, hit_wall};
use crate::theme::{self, Palette};
use crate::tools::wall_tool::{Tool, WallTool};
use crate::view3d::View3d;
use commands::{CommandService, WriterHandle, spawn_writer};
use domain::{ElementId, LengthUnit, Model};
use egui::{Key, KeyboardShortcut, Modifiers};
use persistence::ProjectLock;
use render::SceneRenderer;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

const DOOR_WIDTH_MM: f64 = 800.0;
const DOOR_HEIGHT_MM: f64 = 2100.0;
const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(0xDC, 0x26, 0x26);

const SC_NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const SC_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SC_SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SC_EXPORT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::E);
const SC_UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const SC_REDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const SC_SPACES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::R);
const SC_AI: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::I);

/// Esiti dei lavori di I/O sui thread worker.
enum IoResult {
    Opened(PathBuf, Result<Box<(Model, ProjectLock)>, String>),
    Saved(PathBuf, Result<Option<ProjectLock>, String>),
    Exported(PathBuf, Result<usize, String>),
}

enum Dialog {
    New(LengthUnit),
    Open(String),
    SaveAs(String),
    Export(String),
}

pub struct WakuApp {
    ctl: Controller<WriterHandle>,
    bridge: AiBridge,
    wgpu: Option<egui_wgpu::RenderState>,
    plan: PlanView,
    view3d: View3d,
    tool: Tool,
    wall: WallTool,
    focus_length: bool,
    cursor: Option<[f64; 2]>,
    selected: Option<ElementId>,
    path: Option<PathBuf>,
    lock: Option<ProjectLock>,
    io_tx: Sender<IoResult>,
    io_rx: Receiver<IoResult>,
    io_busy: usize,
    dialog: Option<Dialog>,
    show_ai: bool,
}

fn new_writer(model: Model) -> Result<(WriterHandle, Controller<WriterHandle>), String> {
    let handle = spawn_writer(CommandService::new(model));
    let ctl = Controller::new(handle.clone()).map_err(|e| format!("avvio del modello: {e}"))?;
    Ok((handle, ctl))
}

impl WakuApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        theme::apply(&cc.egui_ctx);
        let wgpu = cc.wgpu_render_state.clone();
        if let Some(rs) = &wgpu {
            let scene = SceneRenderer::new(
                &rs.device,
                rs.target_format,
                Some(egui_wgpu::wgpu::TextureFormat::Depth32Float),
            );
            rs.renderer.write().callback_resources.insert(scene);
        }
        let (handle, ctl) = new_writer(Model::new("Nuovo progetto", LengthUnit::Mm))?;
        let bridge = AiBridge::start(handle);
        let (io_tx, io_rx) = channel();
        Ok(Self {
            ctl,
            bridge,
            wgpu,
            plan: PlanView::default(),
            view3d: View3d::default(),
            tool: Tool::Select,
            wall: WallTool::default(),
            focus_length: false,
            cursor: None,
            selected: None,
            path: None,
            lock: None,
            io_tx,
            io_rx,
            io_busy: 0,
            dialog: None,
            show_ai: true,
        })
    }

    fn unit(&self) -> LengthUnit {
        self.ctl.snapshot().model.project.unit
    }

    fn level(&self) -> Option<ElementId> {
        self.ctl.snapshot().model.levels.keys().next().copied()
    }

    /// Sostituisce il modello con un nuovo writer; in caso di errore il modello
    /// corrente resta attivo e l'errore va in barra di stato.
    fn replace_model(&mut self, model: Model) -> bool {
        let (handle, ctl) = match new_writer(model) {
            Ok(w) => w,
            Err(e) => {
                self.ctl.set_status(e, true);
                return false;
            }
        };
        self.ctl = ctl;
        self.bridge.restart(handle);
        self.view3d.reset();
        self.selected = None;
        self.wall.cancel();
        true
    }

    // ---- I/O su thread worker ----

    fn spawn_io(&mut self, ctx: &egui::Context, job: impl FnOnce() -> IoResult + Send + 'static) {
        let tx = self.io_tx.clone();
        let ctx = ctx.clone();
        self.io_busy += 1;
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
    }

    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.path.as_ref() == Some(&path) {
            self.ctl.set_status("Il progetto e' gia' aperto", true);
            return;
        }
        self.spawn_io(ctx, move || {
            let res = ProjectLock::acquire(&path)
                .map_err(|e| e.to_string())
                .and_then(|lock| {
                    persistence::load(&path)
                        .map(|m| Box::new((m, lock)))
                        .map_err(|e| e.to_string())
                });
            IoResult::Opened(path, res)
        });
    }

    fn save(&mut self, ctx: &egui::Context, path: PathBuf) {
        let need_lock = self.lock.is_none() || self.path.as_ref() != Some(&path);
        let model = Arc::clone(&self.ctl.snapshot().model);
        self.spawn_io(ctx, move || {
            let res = (|| {
                let lock = if need_lock {
                    Some(ProjectLock::acquire(&path).map_err(|e| e.to_string())?)
                } else {
                    None
                };
                persistence::save(&path, &model).map_err(|e| e.to_string())?;
                Ok(lock)
            })();
            IoResult::Saved(path, res)
        });
    }

    fn export(&mut self, ctx: &egui::Context, path: PathBuf) {
        let model = Arc::clone(&self.ctl.snapshot().model);
        self.spawn_io(ctx, move || {
            let res = (|| {
                use std::io::Write;
                let f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                let mut w = std::io::BufWriter::new(f);
                let rep = ifc::export_ifc4(&model, &mut w).map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
                Ok(rep.skipped.len())
            })();
            IoResult::Exported(path, res)
        });
    }

    fn poll_io(&mut self) {
        while let Ok(r) = self.io_rx.try_recv() {
            self.io_busy = self.io_busy.saturating_sub(1);
            match r {
                IoResult::Opened(path, Ok(opened)) => {
                    let (model, lock) = *opened;
                    if !self.replace_model(model) {
                        // `lock` del file non aperto viene rilasciato qui.
                        continue;
                    }
                    // Il lock precedente viene rilasciato qui.
                    self.lock = Some(lock);
                    self.ctl
                        .set_status(format!("Aperto {}", path.display()), false);
                    self.path = Some(path);
                }
                IoResult::Opened(path, Err(e)) => {
                    self.ctl
                        .set_status(format!("Apertura di {} fallita: {e}", path.display()), true);
                }
                IoResult::Saved(path, Ok(lock)) => {
                    if let Some(l) = lock {
                        self.lock = Some(l);
                    }
                    self.ctl
                        .set_status(format!("Salvato {}", path.display()), false);
                    self.path = Some(path);
                }
                IoResult::Saved(path, Err(e)) => {
                    self.ctl.set_status(
                        format!("Salvataggio di {} fallito: {e}", path.display()),
                        true,
                    );
                }
                IoResult::Exported(path, Ok(skipped)) => {
                    let extra = if skipped > 0 {
                        format!(" ({skipped} elementi saltati)")
                    } else {
                        String::new()
                    };
                    self.ctl
                        .set_status(format!("IFC esportato in {}{extra}", path.display()), false);
                }
                IoResult::Exported(path, Err(e)) => {
                    self.ctl.set_status(
                        format!("Export IFC di {} fallito: {e}", path.display()),
                        true,
                    );
                }
            }
        }
    }

    // ---- Azioni ----

    fn request_save(&mut self, ctx: &egui::Context) {
        match self.path.clone() {
            Some(p) => self.save(ctx, p),
            None => self.dialog = Some(Dialog::SaveAs("progetto.waku".to_owned())),
        }
    }

    fn detect_spaces(&mut self) {
        if let Some(l) = self.level() {
            self.ctl.detect_spaces(l);
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected.take() {
            self.ctl.delete_wall(id);
        }
    }

    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.wall.cancel();
    }

    fn commit_wall(&mut self, cursor: [f64; 2]) {
        let unit = self.unit();
        match self.wall.commit(cursor, unit) {
            Ok(d) => {
                if let Some(level) = self.level() {
                    self.ctl
                        .create_wall(level, d.start, d.end, d.thickness_mm, d.height_mm);
                }
            }
            Err(e) => self.ctl.set_status(e, true),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |sc: &KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(sc));
        if pressed(&SC_NEW) {
            self.dialog = Some(Dialog::New(LengthUnit::Mm));
        }
        if pressed(&SC_OPEN) {
            self.dialog = Some(Dialog::Open(String::new()));
        }
        if pressed(&SC_SAVE) {
            self.request_save(ctx);
        }
        if typing {
            // Ctrl+Z/Y/R/E/I e tasti singoli restano al campo di testo attivo.
            return;
        }
        if pressed(&SC_EXPORT) {
            self.dialog = Some(Dialog::Export("modello.ifc".to_owned()));
        }
        if pressed(&SC_SPACES) {
            self.detect_spaces();
        }
        if pressed(&SC_AI) {
            self.show_ai = !self.show_ai;
        }
        if pressed(&SC_UNDO) {
            self.ctl.undo();
        }
        if pressed(&SC_REDO) {
            self.ctl.redo();
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::W) {
            self.set_tool(Tool::Wall);
        }
        if key(Key::D) {
            self.set_tool(Tool::Door);
        }
        if key(Key::Escape) {
            if self.wall.drawing() {
                self.wall.cancel();
            } else {
                self.set_tool(Tool::Select);
            }
        }
        if key(Key::Delete) {
            self.delete_selected();
        }
    }

    // ---- Pannelli ----

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let ctx = ui.ctx().clone();
            ui.menu_button("File", |ui| {
                if ui
                    .add(egui::Button::new("Nuovo").shortcut_text(ctx.format_shortcut(&SC_NEW)))
                    .clicked()
                {
                    self.dialog = Some(Dialog::New(LengthUnit::Mm));
                }
                if ui
                    .add(egui::Button::new("Apri…").shortcut_text(ctx.format_shortcut(&SC_OPEN)))
                    .clicked()
                {
                    self.dialog = Some(Dialog::Open(String::new()));
                }
                if ui
                    .add(egui::Button::new("Salva").shortcut_text(ctx.format_shortcut(&SC_SAVE)))
                    .clicked()
                {
                    self.request_save(&ctx);
                }
                if ui.button("Salva con nome…").clicked() {
                    self.dialog = Some(Dialog::SaveAs("progetto.waku".to_owned()));
                }
                if ui
                    .add(
                        egui::Button::new("Esporta IFC…")
                            .shortcut_text(ctx.format_shortcut(&SC_EXPORT)),
                    )
                    .clicked()
                {
                    self.dialog = Some(Dialog::Export("modello.ifc".to_owned()));
                }
                ui.separator();
                if ui.button("Esci").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Modifica", |ui| {
                if ui
                    .add(egui::Button::new("Annulla").shortcut_text(ctx.format_shortcut(&SC_UNDO)))
                    .clicked()
                {
                    self.ctl.undo();
                }
                if ui
                    .add(egui::Button::new("Ripeti").shortcut_text(ctx.format_shortcut(&SC_REDO)))
                    .clicked()
                {
                    self.ctl.redo();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("Elimina muro").shortcut_text("Canc"),
                    )
                    .clicked()
                {
                    self.delete_selected();
                }
            });
            ui.menu_button("Visualizza", |ui| {
                if ui
                    .add(
                        egui::Button::selectable(self.show_ai, "Pannello Accesso AI")
                            .shortcut_text(ctx.format_shortcut(&SC_AI)),
                    )
                    .clicked()
                {
                    self.show_ai = !self.show_ai;
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let dark = ui.visuals().dark_mode;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 28.0), egui::Sense::hover());
            theme::paint_logo(ui.painter(), rect.min, 28.0, dark);
            ui.separator();
            for (t, label, key) in [
                (Tool::Select, "Selezione", "Esc"),
                (Tool::Wall, "Muro", "W"),
                (Tool::Door, "Porta", "D"),
            ] {
                if ui
                    .add(egui::Button::selectable(self.tool == t, label))
                    .on_hover_text(format!("Scorciatoia: {key}"))
                    .clicked()
                {
                    self.set_tool(t);
                }
            }
            ui.separator();
            if ui
                .button("Rileva vani")
                .on_hover_text("Scorciatoia: Ctrl+R")
                .clicked()
            {
                self.detect_spaces();
            }
            if self.tool == Tool::Wall {
                ui.separator();
                let unit = self.unit().suffix();
                ui.label(format!("Lunghezza ({unit}):"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.wall.length_text)
                        .desired_width(90.0)
                        .hint_text("es. 332-60"),
                );
                if self.focus_length {
                    r.request_focus();
                    self.focus_length = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && self.wall.drawing()
                {
                    let start = self.wall.start.unwrap_or_default();
                    let cursor = self.cursor.unwrap_or([start[0] + 1.0, start[1]]);
                    self.commit_wall(cursor);
                }
                ui.label("Spessore:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.wall.thickness_text).desired_width(70.0),
                );
                ui.label("Altezza:");
                ui.add(egui::TextEdit::singleline(&mut self.wall.height_text).desired_width(70.0));
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match self.ctl.status() {
                Some(s) if s.error => {
                    ui.colored_label(ERROR_COLOR, &s.text);
                }
                Some(s) => {
                    ui.label(&s.text);
                }
                None => {
                    ui.label("Pronto");
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let snap = self.ctl.snapshot();
                ui.label(format!(
                    "Rev. {} · unità {} · {}",
                    snap.revision.0,
                    snap.model.project.unit.suffix(),
                    self.path
                        .as_ref()
                        .map_or("non salvato".to_owned(), |p| p.display().to_string())
                ));
                if self.io_busy > 0 {
                    ui.spinner();
                }
                if let Some(s) = &self.view3d.last_stats {
                    ui.label(format!(
                        "GPU: {} caricate, {} riusate",
                        s.uploaded, s.reused
                    ));
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let mut open = true;
        let mut done = false;
        let title = match &d {
            Dialog::New(_) => "Nuovo progetto",
            Dialog::Open(_) => "Apri progetto",
            Dialog::SaveAs(_) => "Salva con nome",
            Dialog::Export(_) => "Esporta IFC4",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| match &mut d {
                Dialog::New(unit) => {
                    ui.label("Unità di progetto:");
                    ui.horizontal(|ui| {
                        for u in [LengthUnit::Mm, LengthUnit::Cm, LengthUnit::M] {
                            ui.radio_value(unit, u, u.suffix());
                        }
                    });
                    if ui.button("CREA").clicked() {
                        let unit = *unit;
                        if self.replace_model(Model::new("Nuovo progetto", unit)) {
                            self.lock = None;
                            self.path = None;
                            self.ctl.set_status("Nuovo progetto", false);
                        }
                        done = true;
                    }
                }
                Dialog::Open(p) | Dialog::SaveAs(p) | Dialog::Export(p) => {
                    ui.label("Percorso del file:");
                    let r = ui.add(egui::TextEdit::singleline(p).desired_width(360.0));
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if (ui.button("OK").clicked() || enter) && !p.trim().is_empty() {
                        done = true;
                    }
                }
            });
        if done {
            match d {
                Dialog::New(_) => {}
                Dialog::Open(p) => self.open(ctx, PathBuf::from(p.trim())),
                Dialog::SaveAs(p) => self.save(ctx, PathBuf::from(p.trim())),
                Dialog::Export(p) => self.export(ctx, PathBuf::from(p.trim())),
            }
        } else if open {
            self.dialog = Some(d);
        }
    }

    fn plan_panel(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::new(ui.visuals().dark_mode);
        let unit = self.unit();
        let preview = self
            .cursor
            .and_then(|c| self.wall.preview(c, unit))
            .filter(|_| self.tool == Tool::Wall);
        let model = Arc::clone(&self.ctl.snapshot().model);
        let input = self.plan.show(ui, &model, &palette, self.selected, preview);
        if input.hover.is_some() {
            self.cursor = input.hover;
        }
        let Some(p) = input.clicked else {
            return;
        };
        let tol = 6.0 * input.mm_per_px;
        match self.tool {
            Tool::Select => self.selected = hit_wall(&model, p, tol).map(|h| h.0),
            Tool::Wall => {
                if self.wall.drawing() {
                    self.commit_wall(p);
                } else {
                    self.wall.start = Some(p);
                    self.focus_length = true;
                }
            }
            Tool::Door => match hit_wall(&model, p, tol) {
                Some((id, along)) => {
                    let len = model.walls[&id].length().as_mm();
                    let off = door_offset(along, len, DOOR_WIDTH_MM);
                    self.ctl.add_door(id, off, DOOR_WIDTH_MM, DOOR_HEIGHT_MM);
                }
                None => self.ctl.set_status("Nessun muro nel punto cliccato", true),
            },
        }
    }
}

impl eframe::App for WakuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_io();
        if self.ctl.poll() {
            let model = &self.ctl.snapshot().model;
            if self
                .selected
                .is_some_and(|id| !model.walls.contains_key(&id))
            {
                self.selected = None;
            }
        }
        if let Some(rs) = &self.wgpu {
            let snap = self.ctl.snapshot().clone();
            self.view3d.sync(rs, &snap);
        }
        self.shortcuts(&ctx);
        self.dialogs(&ctx);

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu(ui);
            self.toolbar(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.show_ai {
            egui::Panel::right("ai")
                .resizable(true)
                .default_size(320.0)
                .show(ui, |ui| self.bridge.ui(ui, self.ctl.journal()));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let available = self.wgpu.is_some();
            ui.columns(2, |cols| {
                cols[0].label("Pianta");
                self.plan_panel(&mut cols[0]);
                cols[1].label("Vista 3D");
                self.view3d.show(&mut cols[1], available);
            });
        });

        // Snapshot dell'AI e risposte del writer arrivano da altri thread: polling leggero.
        let wait = if self.ctl.busy() || self.io_busy > 0 {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(250)
        };
        ctx.request_repaint_after(wait);
    }

    fn on_exit(&mut self) {
        self.bridge.shutdown();
        self.lock = None;
    }
}
