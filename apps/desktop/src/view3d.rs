//! Vista 3D: `SceneRenderer` in `CallbackResources`, upload incrementale
//! (riusa i buffer GPU dei muri con contenuto invariato).

use commands::{Revision, Snapshot};
use egui::Sense;
use render::{OrbitCamera, SceneCallback, SceneRenderer, UploadStats};

#[derive(Default)]
pub struct View3d {
    camera: OrbitCamera,
    uploaded: Option<Revision>,
    centered: bool,
    pub last_stats: Option<UploadStats>,
}

impl View3d {
    /// Dimentica lo stato (nuovo progetto): al prossimo frame si ricarica tutto.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Carica sulla GPU le mesh del modello se la revisione e' cambiata.
    pub fn sync(&mut self, rs: &egui_wgpu::RenderState, snap: &Snapshot) {
        if self.uploaded == Some(snap.revision) {
            return;
        }
        let meshes = geometry::model_meshes(&snap.model);
        let mut renderer = rs.renderer.write();
        let Some(scene) = renderer.callback_resources.get_mut::<SceneRenderer>() else {
            return;
        };
        self.last_stats = Some(scene.upload(&rs.device, &rs.queue, &meshes));
        if !self.centered && !meshes.is_empty() {
            self.camera.target = scene.origin();
            self.centered = true;
        }
        self.uploaded = Some(snap.revision);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, available: bool) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        if !available {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Vista 3D non disponibile (wgpu non inizializzato)",
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
            return;
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            self.camera.yaw -= f64::from(d.x) * 0.01;
            self.camera.pitch = (self.camera.pitch + f64::from(d.y) * 0.01)
                .clamp(-render::camera::MAX_PITCH, render::camera::MAX_PITCH);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.distance =
                    (self.camera.distance * f64::from((-scroll * 0.002).exp())).clamp(500.0, 1.0e6);
            }
        }
        let aspect = if rect.height() > 0.0 {
            rect.width() / rect.height()
        } else {
            1.0
        };
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            SceneCallback {
                camera: self.camera,
                aspect,
            },
        ));
    }
}
