//! Vista pianta 2D disegnata con il painter egui (Y modello verso l'alto).

use crate::theme::Palette;
use domain::{ElementId, Model};
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

/// Pan/zoom della pianta.
pub struct PlanView {
    /// Pixel per millimetro.
    pub scale: f32,
    /// Posizione schermo dell'origine modello rispetto al centro della vista.
    pub pan: Vec2,
}

impl Default for PlanView {
    fn default() -> Self {
        Self {
            scale: 0.05,
            pan: Vec2::new(-100.0, 75.0),
        }
    }
}

/// Esito dell'interazione con la pianta in questo frame.
pub struct PlanInput {
    /// Punto del cursore in coordinate modello (mm), se sopra la vista.
    pub hover: Option<[f64; 2]>,
    /// Click primario in coordinate modello.
    pub clicked: Option<[f64; 2]>,
    /// Px -> mm, per tolleranze di selezione.
    pub mm_per_px: f64,
}

/// Muro sotto il punto `p` (entro meta' spessore + `tol_mm`), con la posizione
/// lungo l'asse (mm dall'inizio). Restituisce il piu' vicino.
pub fn hit_wall(model: &Model, p: [f64; 2], tol_mm: f64) -> Option<(ElementId, f64)> {
    let mut best: Option<(ElementId, f64, f64)> = None;
    for (id, w) in &model.walls {
        let (ax, ay) = (w.start.x, w.start.y);
        let (dx, dy) = (w.end.x - ax, w.end.y - ay);
        let len2 = dx * dx + dy * dy;
        if len2 <= f64::EPSILON {
            continue;
        }
        let t = (((p[0] - ax) * dx + (p[1] - ay) * dy) / len2).clamp(0.0, 1.0);
        let (cx, cy) = (ax + t * dx, ay + t * dy);
        let dist = (p[0] - cx).hypot(p[1] - cy);
        if dist <= w.thickness.as_mm() / 2.0 + tol_mm && best.is_none_or(|b| dist < b.2) {
            best = Some((*id, t * len2.sqrt(), dist));
        }
    }
    best.map(|(id, along, _)| (id, along))
}

/// Offset della porta (bordo iniziale) centrata sul punto cliccato, entro il muro.
pub fn door_offset(along_mm: f64, wall_len_mm: f64, width_mm: f64) -> f64 {
    (along_mm - width_mm / 2.0).clamp(0.0, (wall_len_mm - width_mm).max(0.0))
}

impl PlanView {
    fn to_screen(&self, rect: Rect, p: [f64; 2]) -> Pos2 {
        let c = rect.center() + self.pan;
        Pos2::new(
            c.x + p[0] as f32 * self.scale,
            c.y - p[1] as f32 * self.scale,
        )
    }

    fn to_world(&self, rect: Rect, s: Pos2) -> [f64; 2] {
        let c = rect.center() + self.pan;
        [
            f64::from((s.x - c.x) / self.scale),
            f64::from((c.y - s.y) / self.scale),
        ]
    }

    /// Disegna il modello e restituisce l'input (click/hover) in coordinate modello.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        palette: &Palette,
        selected: Option<ElementId>,
        preview: Option<([f64; 2], [f64; 2])>,
    ) -> PlanInput {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.canvas);

        // Pan con trascinamento secondario/centrale, zoom con rotella attorno al cursore.
        if resp.dragged_by(egui::PointerButton::Secondary)
            || resp.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += resp.drag_delta();
        }
        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let before = self.to_world(rect, hp);
                self.scale = (self.scale * (scroll * 0.002).exp()).clamp(0.002, 2.0);
                let after = self.to_screen(rect, before);
                self.pan += hp - after;
            }
        }

        // Griglia 1 m.
        let grid = Stroke::new(1.0, palette.grid);
        let step = 1000.0;
        let tl = self.to_world(rect, rect.left_top());
        let br = self.to_world(rect, rect.right_bottom());
        if self.scale * step as f32 >= 8.0 {
            let mut x = (tl[0] / step).floor() * step;
            while x <= br[0] {
                let s = self.to_screen(rect, [x, 0.0]).x;
                painter.vline(s, rect.y_range(), grid);
                x += step;
            }
            let mut y = (br[1] / step).floor() * step;
            while y <= tl[1] {
                let s = self.to_screen(rect, [0.0, y]).y;
                painter.hline(rect.x_range(), s, grid);
                y += step;
            }
        }

        for s in model.spaces.values() {
            if let Some(poly) = geometry::space_net_polygon(model, &s.boundary) {
                let pts: Vec<Pos2> = poly
                    .iter()
                    .map(|p| self.to_screen(rect, [p.x, p.y]))
                    .collect();
                // Poligono potenzialmente concavo: solo contorno + etichetta.
                painter.add(egui::Shape::closed_line(
                    pts.clone(),
                    Stroke::new(1.0, palette.space),
                ));
                let n = pts.len().max(1) as f32;
                let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n;
                painter.text(
                    c.to_pos2(),
                    egui::Align2::CENTER_CENTER,
                    &s.name,
                    egui::FontId::proportional(13.0),
                    palette.space,
                );
            }
        }

        for id in model.walls.keys() {
            let fp = geometry::wall_footprint(model, *id);
            let pts: Vec<Pos2> = fp
                .iter()
                .map(|p| self.to_screen(rect, [p.x, p.y]))
                .collect();
            let fill = if Some(*id) == selected {
                palette.accent
            } else {
                palette.wall
            };
            painter.add(egui::Shape::convex_polygon(
                pts,
                fill,
                Stroke::new(1.0, palette.text),
            ));
        }

        for o in model.openings.values() {
            let Some(w) = model.walls.get(&o.host) else {
                continue;
            };
            let len = w.length().as_mm();
            if len <= 0.0 {
                continue;
            }
            let (dx, dy) = ((w.end.x - w.start.x) / len, (w.end.y - w.start.y) / len);
            let a = o.offset.as_mm();
            let b = a + o.width.as_mm();
            let p0 = self.to_screen(rect, [w.start.x + dx * a, w.start.y + dy * a]);
            let p1 = self.to_screen(rect, [w.start.x + dx * b, w.start.y + dy * b]);
            let px = (w.thickness.as_mm() as f32 * self.scale).max(3.0);
            painter.line_segment([p0, p1], Stroke::new(px, palette.canvas));
            painter.line_segment([p0, p1], Stroke::new(2.0, palette.accent));
        }

        if let Some((a, b)) = preview {
            let (pa, pb) = (self.to_screen(rect, a), self.to_screen(rect, b));
            painter.line_segment([pa, pb], Stroke::new(2.0, palette.accent));
            painter.circle_filled(pa, 3.0, palette.accent);
        }

        let hover = resp.hover_pos().map(|p| self.to_world(rect, p));
        let clicked = if resp.clicked() {
            resp.interact_pointer_pos().map(|p| self.to_world(rect, p))
        } else {
            None
        };
        PlanInput {
            hover,
            clicked,
            mm_per_px: f64::from(1.0 / self.scale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::fixtures::sample_room;

    #[test]
    fn hit_wall_finds_nearest_and_offset() {
        let (m, ids) = sample_room();
        let (id, along) = hit_wall(&m, [1000.0, 30.0], 0.0).unwrap();
        assert_eq!(id, ids.walls[0]);
        assert!((along - 1000.0).abs() < 1e-9);
        assert!(hit_wall(&m, [2000.0, 1500.0], 10.0).is_none());
    }

    #[test]
    fn door_offset_clamped_inside_wall() {
        assert_eq!(door_offset(1000.0, 4000.0, 800.0), 600.0);
        assert_eq!(door_offset(100.0, 4000.0, 800.0), 0.0);
        assert_eq!(door_offset(3990.0, 4000.0, 800.0), 3200.0);
    }
}
