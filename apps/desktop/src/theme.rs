//! Palette Waku (PDR §1.3, REQ-BRAND-02) e logo disegnato con il painter egui.

use crate::app::{LogoVariant, logo_variant};
use egui::{Color32, Pos2, Rect, Stroke, Vec2};

/// Colori delle quattro facce del simbolo, tema chiaro.
pub const FACES_LIGHT: [Color32; 4] = [
    Color32::from_rgb(0xCC, 0x63, 0x3D),
    Color32::from_rgb(0xAD, 0x49, 0x2B),
    Color32::from_rgb(0xBE, 0x55, 0x32),
    Color32::from_rgb(0xC6, 0x5A, 0x34),
];
/// Colori delle quattro facce del simbolo, tema scuro.
pub const FACES_DARK: [Color32; 4] = [
    Color32::from_rgb(0xE0, 0x75, 0x4D),
    Color32::from_rgb(0xC2, 0x5B, 0x3A),
    Color32::from_rgb(0xD2, 0x66, 0x3F),
    Color32::from_rgb(0xD9, 0x6C, 0x45),
];
pub const TEXT_LIGHT: Color32 = Color32::from_rgb(0x30, 0x27, 0x20);
pub const TEXT_DARK: Color32 = Color32::from_rgb(0xF3, 0xEB, 0xE3);
pub const BG_LIGHT: Color32 = Color32::from_rgb(0xFA, 0xF7, 0xF2);
pub const BG_DARK: Color32 = Color32::from_rgb(0x1C, 0x17, 0x14);

/// Colori derivati per le viste.
pub struct Palette {
    pub canvas: Color32,
    pub grid: Color32,
    pub wall: Color32,
    pub text: Color32,
    pub accent: Color32,
    pub space: Color32,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                canvas: BG_DARK,
                grid: Color32::from_rgb(0x30, 0x29, 0x24),
                wall: Color32::from_rgb(0x8A, 0x7E, 0x74),
                text: TEXT_DARK,
                accent: FACES_DARK[0],
                space: Color32::from_rgb(0xB8, 0xAC, 0xA0),
            }
        } else {
            Self {
                canvas: BG_LIGHT,
                grid: Color32::from_rgb(0xEA, 0xE4, 0xDB),
                wall: Color32::from_rgb(0x9A, 0x8E, 0x84),
                text: TEXT_LIGHT,
                accent: FACES_LIGHT[0],
                space: Color32::from_rgb(0x6B, 0x5E, 0x54),
            }
        }
    }
}

/// Applica sfondi e testo del brand ai due temi.
pub fn apply(ctx: &egui::Context) {
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.style_mut_of(theme, |s| {
            let (bg, text, accent) = if dark {
                (BG_DARK, TEXT_DARK, FACES_DARK[0])
            } else {
                (BG_LIGHT, TEXT_LIGHT, FACES_LIGHT[0])
            };
            s.visuals.panel_fill = bg;
            s.visuals.window_fill = bg;
            s.visuals.override_text_color = Some(text);
            s.visuals.selection.bg_fill = accent;
            s.visuals.hyperlink_color = accent;
        });
    }
}

/// Facce del simbolo (volume a 4 facce) in un box 32x32; stesso disegno degli SVG.
const SYMBOL: [&[[f32; 2]]; 4] = [
    // falda sinistra
    &[[4.0, 12.0], [16.0, 6.0], [16.0, 18.0]],
    // fronte sinistro
    &[[4.0, 12.0], [16.0, 18.0], [16.0, 30.0], [4.0, 24.0]],
    // fronte destro
    &[[16.0, 18.0], [28.0, 12.0], [28.0, 24.0], [16.0, 30.0]],
    // falda destra
    &[[16.0, 6.0], [28.0, 12.0], [16.0, 18.0]],
];

/// Disegna il logo con angolo in alto a sinistra `origin` e altezza `height`;
/// restituisce il rettangolo occupato.
pub fn paint_logo(painter: &egui::Painter, origin: Pos2, height: f32, dark: bool) -> Rect {
    let variant = logo_variant(dark, height);
    let (faces, text) = match variant {
        LogoVariant::FullLight | LogoVariant::SymbolLight => (FACES_LIGHT, TEXT_LIGHT),
        LogoVariant::FullDark | LogoVariant::SymbolDark => (FACES_DARK, TEXT_DARK),
    };
    let k = height / 32.0;
    for (face, color) in SYMBOL.iter().zip(faces) {
        let pts: Vec<Pos2> = face
            .iter()
            .map(|p| origin + Vec2::new(p[0] * k, p[1] * k))
            .collect();
        painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
    }
    let mut width = height;
    if matches!(variant, LogoVariant::FullLight | LogoVariant::FullDark) {
        let r = painter.text(
            origin + Vec2::new(height * 1.2, height * 0.5),
            egui::Align2::LEFT_CENTER,
            "waku",
            egui::FontId::proportional(height * 0.75),
            text,
        );
        width = r.right() - origin.x;
    }
    Rect::from_min_size(origin, Vec2::new(width, height))
}
