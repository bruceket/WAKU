//! Palette ufficiale Waku (REQ-BRAND-02), logo disegnato con il painter egui e
//! icona finestra rasterizzata in-crate (nessuna dipendenza aggiuntiva).
//!
//! Geometria ricavata da `assets/logo-light.svg` e `assets/app-icon.svg`.

use crate::app::{LogoVariant, logo_variant};
use egui::{Color32, Pos2, Rect, Stroke, Vec2};

/// Primario ufficiale (quadrato del simbolo, evidenziazione 3D, strumento corrente).
pub const PRIMARY: Color32 = Color32::from_rgb(0xBE, 0x55, 0x32);
/// Primario del tema scuro (wordmark scuro, selezione, link).
pub const PRIMARY_DARK: Color32 = Color32::from_rgb(0xE0, 0x75, 0x4D);
/// Variante più scura del primario per superfici con testo chiaro sopra
/// (pulsanti attivi, selezione, link su sfondo chiaro): `#FAF7F2` su `#BE5532`
/// misura 4,33:1 (< 4,5 WCAG AA); `#B95331` è la tonalità più vicina che supera 4,5:1 (4,52:1).
pub const PRIMARY_STRONG: Color32 = Color32::from_rgb(0xB9, 0x53, 0x31);
pub const TEXT_LIGHT: Color32 = Color32::from_rgb(0x30, 0x27, 0x20);
pub const TEXT_DARK: Color32 = Color32::from_rgb(0xF3, 0xEB, 0xE3);
pub const BG_LIGHT: Color32 = Color32::from_rgb(0xFA, 0xF7, 0xF2);
pub const BG_DARK: Color32 = Color32::from_rgb(0x1C, 0x17, 0x14);
/// Colore delle facce del simbolo.
pub const FACE: Color32 = BG_LIGHT;
/// Opacità delle quattro facce (SVG `fill-opacity`).
pub const FACE_OPACITY: [f32; 4] = [1.0, 0.55, 0.8, 0.92];

/// Rapporto di contrasto WCAG 2.1 fra due colori sRGB (1..=21).
#[cfg_attr(not(test), expect(dead_code))]
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    fn lum(c: [u8; 3]) -> f64 {
        let ch = |v: u8| {
            let s = f64::from(v) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
    }
    let (la, lb) = (lum(a), lum(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

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
                accent: active_fill(true),
                space: Color32::from_rgb(0xB8, 0xAC, 0xA0),
            }
        } else {
            Self {
                canvas: BG_LIGHT,
                grid: Color32::from_rgb(0xEA, 0xE4, 0xDB),
                wall: Color32::from_rgb(0x9A, 0x8E, 0x84),
                text: TEXT_LIGHT,
                accent: active_fill(false),
                space: Color32::from_rgb(0x6B, 0x5E, 0x54),
            }
        }
    }
}

/// Colori di stato per un tema: distinti dal primario, >= 4,5:1 sullo sfondo.
pub struct StatusColors {
    pub error: Color32,
    pub warning: Color32,
    pub ok: Color32,
}

pub fn status_colors(dark: bool) -> StatusColors {
    if dark {
        StatusColors {
            error: Color32::from_rgb(0xFB, 0x71, 0x85),
            warning: Color32::from_rgb(0xFB, 0xBF, 0x24),
            ok: Color32::from_rgb(0x4A, 0xDE, 0x80),
        }
    } else {
        StatusColors {
            error: Color32::from_rgb(0xBE, 0x12, 0x3C),
            warning: Color32::from_rgb(0x7C, 0x5E, 0x00),
            ok: Color32::from_rgb(0x15, 0x80, 0x3D),
        }
    }
}

/// Riempimento di stati attivi, selezione e strumento corrente (token unico).
pub fn active_fill(dark: bool) -> Color32 {
    if dark { PRIMARY_DARK } else { PRIMARY_STRONG }
}

/// Visuals egui con la palette ufficiale. Nessun `override_text_color`: in egui
/// 0.37 vincerebbe su `fg_stroke` degli stati attivi e su `selection.stroke`.
pub fn visuals(dark: bool) -> egui::Visuals {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let (bg, text) = if dark {
        (BG_DARK, TEXT_DARK)
    } else {
        (BG_LIGHT, TEXT_LIGHT)
    };
    let fill = active_fill(dark);
    // etichetta sul riempimento attivo: #FAF7F2 su #B95331, #1C1714 su #E0754D
    let on_fill = if dark { BG_DARK } else { BG_LIGHT };
    let status = status_colors(dark);
    v.panel_fill = bg;
    v.window_fill = bg;
    v.override_text_color = None;
    v.widgets.noninteractive.fg_stroke.color = text;
    v.widgets.inactive.fg_stroke.color = text;
    v.widgets.hovered.fg_stroke.color = text;
    v.widgets.open.fg_stroke.color = text;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, fill);
    v.widgets.active.bg_fill = fill;
    v.widgets.active.weak_bg_fill = fill;
    v.widgets.active.bg_stroke = Stroke::new(1.0, fill);
    v.widgets.active.fg_stroke = Stroke::new(1.5, on_fill);
    v.selection.bg_fill = fill;
    v.selection.stroke = Stroke::new(1.0, on_fill);
    v.hyperlink_color = fill;
    v.error_fg_color = status.error;
    v.warn_fg_color = status.warning;
    v
}

/// Applica la palette ufficiale ai due temi egui.
pub fn apply(ctx: &egui::Context) {
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.style_mut_of(theme, |s| s.visuals = visuals(dark));
    }
}

/// Le quattro facce del simbolo nel sistema dell'SVG (prima delle trasformazioni).
const FACES: [[[f32; 2]; 4]; 4] = [
    [[35.0, 40.0], [90.0, 72.0], [90.0, 202.0], [35.0, 170.0]],
    [[90.0, 119.0], [146.0, 80.0], [146.0, 163.0], [90.0, 202.0]],
    [
        [146.0, 80.0],
        [203.0, 119.0],
        [203.0, 202.0],
        [146.0, 163.0],
    ],
    [[203.0, 72.0], [259.0, 37.0], [259.0, 167.0], [203.0, 202.0]],
];

/// Facce trasformate come `translate(c) scale(s) translate(-147 -120)`.
pub fn faces_at(center: [f32; 2], scale: f32) -> [[[f32; 2]; 4]; 4] {
    FACES.map(|f| {
        f.map(|p| {
            [
                center[0] + scale * (p[0] - 147.0),
                center[1] + scale * (p[1] - 120.0),
            ]
        })
    })
}

/// Poligono di un rettangolo arrotondato (archi a `seg` segmenti per angolo).
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, seg: usize) -> Vec<[f32; 2]> {
    let corners = [
        (x + w - r, y + r, -0.5f32),
        (x + w - r, y + h - r, 0.0),
        (x + r, y + h - r, 0.5),
        (x + r, y + r, 1.0),
    ];
    let mut out = Vec::with_capacity(4 * (seg + 1));
    for (cx, cy, start) in corners {
        for i in 0..=seg {
            let t = std::f32::consts::PI * (start + 0.5 * i as f32 / seg as f32);
            out.push([cx + r * t.cos(), cy + r * t.sin()]);
        }
    }
    out
}

/// Tratti del wordmark "waku" nel sistema del logo (viewBox 803x240), già
/// trasformati con `matrix(1.1429 0 0 1.1429 -93.14 -28.57)`; spessore [`WORD_STROKE`].
pub fn wordmark_strokes() -> Vec<Vec<[f32; 2]>> {
    use std::f32::consts::PI;
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize| -> Vec<[f32; 2]> {
        (0..=n)
            .map(|i| {
                let t = a0 + (a1 - a0) * i as f32 / n as f32;
                [cx + r * t.cos(), cy + r * t.sin()]
            })
            .collect()
    };
    let mut u = vec![[661.4, 112.0]];
    u.extend(arc(699.4, 150.0, 38.0, PI, 0.0, 16));
    let raw: Vec<Vec<[f32; 2]>> = vec![
        // w
        vec![
            [300.0, 112.0],
            [330.0, 188.0],
            [360.0, 112.0],
            [390.0, 188.0],
            [420.0, 112.0],
        ],
        // a: cerchio + asta
        arc(486.5, 150.0, 38.0, 0.0, 2.0 * PI, 32),
        vec![[524.5, 112.0], [524.5, 188.0]],
        // k: asta + due diagonali
        vec![[577.2, 72.0], [577.2, 188.0]],
        vec![[627.2, 114.0], [577.2, 152.0], [629.2, 188.0]],
        // u: asta, arco, asta
        u,
        vec![[737.4, 112.0], [737.4, 188.0]],
    ];
    raw.into_iter()
        .map(|s| {
            s.into_iter()
                .map(|[x, y]| [1.1429 * x - 93.14, 1.1429 * y - 28.57])
                .collect()
        })
        .collect()
}

/// Spessore del tratto del wordmark nel sistema del logo.
pub const WORD_STROKE: f32 = 24.0 * 1.1429;
/// Estremo destro del logo completo (wordmark + mezzo tratto) nel viewBox.
const LOGO_RIGHT: f32 = 737.4 * 1.1429 - 93.14 + WORD_STROKE / 2.0;

/// Disegna il logo con angolo in alto a sinistra `origin` e altezza `height`;
/// restituisce il rettangolo occupato. Il simbolo occupa `height` x `height`.
pub fn paint_logo(painter: &egui::Painter, origin: Pos2, height: f32, dark: bool) -> Rect {
    let variant = logo_variant(dark, height);
    // viewBox: il quadrato va da 40 a 200 → scala k, origine (40, 40).
    let k = height / 160.0;
    let map = |p: [f32; 2]| origin + Vec2::new((p[0] - 40.0) * k, (p[1] - 40.0) * k);
    let rr: Vec<Pos2> = rounded_rect(40.0, 40.0, 160.0, 160.0, 36.0, 6)
        .into_iter()
        .map(map)
        .collect();
    painter.add(egui::Shape::convex_polygon(rr, PRIMARY, Stroke::NONE));
    for (face, a) in faces_at([120.0, 120.0], 0.5).iter().zip(FACE_OPACITY) {
        let pts: Vec<Pos2> = face.iter().map(|p| map(*p)).collect();
        painter.add(egui::Shape::convex_polygon(
            pts,
            FACE.gamma_multiply(a),
            Stroke::NONE,
        ));
    }
    let mut width = height;
    if matches!(variant, LogoVariant::FullLight | LogoVariant::FullDark) {
        let color = if dark { PRIMARY_DARK } else { PRIMARY };
        let w = WORD_STROKE * k;
        for s in wordmark_strokes() {
            let pts: Vec<Pos2> = s.into_iter().map(map).collect();
            // estremi e giunzioni arrotondati (stroke-linecap/linejoin round)
            for p in &pts {
                painter.circle_filled(*p, w / 2.0, color);
            }
            painter.add(egui::Shape::line(pts, Stroke::new(w, color)));
        }
        width = (LOGO_RIGHT - 40.0) * k;
    }
    Rect::from_min_size(origin, Vec2::new(width, height))
}

/// Riempie `poly` (pari-dispari, campionamento 4x4 per pixel) su un buffer RGBA
/// `size`x`size` con colore `rgb` e opacità `alpha` (composizione "over").
fn fill_polygon(buf: &mut [u8], size: usize, poly: &[[f32; 2]], rgb: [u8; 3], alpha: f32) {
    const SS: usize = 4;
    let mut xs = Vec::new();
    for py in 0..size {
        let mut cover = vec![0u8; size];
        for sy in 0..SS {
            let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
            xs.clear();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a[1] <= y) != (b[1] <= y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                for (px, c) in cover.iter_mut().enumerate() {
                    for sx in 0..SS {
                        let x = px as f32 + (sx as f32 + 0.5) / SS as f32;
                        if x >= pair[0] && x < pair[1] {
                            *c += 1;
                        }
                    }
                }
            }
        }
        for (px, &c) in cover.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let sa = alpha * f32::from(c) / (SS * SS) as f32;
            let o = (py * size + px) * 4;
            let da = f32::from(buf[o + 3]) / 255.0;
            let oa = sa + da * (1.0 - sa);
            for ch in 0..3 {
                let d = f32::from(buf[o + ch]);
                let v = (f32::from(rgb[ch]) * sa + d * da * (1.0 - sa)) / oa;
                buf[o + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
            buf[o + 3] = (oa * 255.0).round() as u8;
        }
    }
}

/// Lato in pixel dell'icona finestra.
pub const ICON_SIZE: usize = 256;

/// Icona finestra RGBA `ICON_SIZE`x`ICON_SIZE` rasterizzata da `assets/app-icon.svg`.
pub fn app_icon_rgba() -> Vec<u8> {
    let n = ICON_SIZE;
    let s = n as f32 / 512.0;
    let mut buf = vec![0u8; n * n * 4];
    let rr: Vec<[f32; 2]> = rounded_rect(0.0, 0.0, 512.0, 512.0, 115.0, 16)
        .into_iter()
        .map(|[x, y]| [x * s, y * s])
        .collect();
    fill_polygon(&mut buf, n, &rr, [0xBE, 0x55, 0x32], 1.0);
    for (face, a) in faces_at([256.0, 256.0], 1.6).iter().zip(FACE_OPACITY) {
        let poly: Vec<[f32; 2]> = face.iter().map(|[x, y]| [x * s, y * s]).collect();
        fill_polygon(&mut buf, n, &poly, [0xFA, 0xF7, 0xF2], a);
    }
    buf
}

/// Icona finestra per `ViewportBuilder::with_icon`.
pub fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: app_icon_rgba(),
        width: ICON_SIZE as u32,
        height: ICON_SIZE as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(c: Color32) -> [u8; 3] {
        [c.r(), c.g(), c.b()]
    }

    fn hue(c: Color32) -> f32 {
        let (r, g, b) = (f32::from(c.r()), f32::from(c.g()), f32::from(c.b()));
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let d = mx - mn;
        if d == 0.0 {
            0.0
        } else if mx == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if mx == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        }
    }

    #[test]
    fn visuals_rendered_pairs_aa() {
        for dark in [false, true] {
            let v = visuals(dark);
            assert!(v.override_text_color.is_none());
            let a = &v.widgets.active;
            let w = &v.widgets;
            let pairs = [
                ("attivo/bg", a.fg_stroke.color, a.bg_fill),
                ("attivo/weak", a.fg_stroke.color, a.weak_bg_fill),
                ("selezione", v.selection.stroke.color, v.selection.bg_fill),
                (
                    "testo/pannello",
                    w.noninteractive.fg_stroke.color,
                    v.panel_fill,
                ),
                (
                    "inattivo",
                    w.inactive.fg_stroke.color,
                    w.inactive.weak_bg_fill,
                ),
                ("hover", w.hovered.fg_stroke.color, w.hovered.weak_bg_fill),
                ("link", v.hyperlink_color, v.panel_fill),
            ];
            for (name, fg, bg) in pairs {
                let r = contrast_ratio(rgb(fg), rgb(bg));
                println!("dark={dark} {name}: {r:.2}");
                assert!(r >= 4.5, "dark={dark} {name}: {r:.2}");
            }
        }
    }

    #[test]
    fn status_colors_aa_and_distinct() {
        for dark in [false, true] {
            let bg = if dark { BG_DARK } else { BG_LIGHT };
            let s = status_colors(dark);
            let primary = active_fill(dark);
            for (name, c) in [("errore", s.error), ("avviso", s.warning), ("ok", s.ok)] {
                let r = contrast_ratio(rgb(c), rgb(bg));
                let dh = (hue(c) - hue(primary)).abs();
                let dh = dh.min(360.0 - dh);
                println!("dark={dark} {name}: {r:.2}, delta tinta {dh:.0}");
                assert!(r >= 4.5, "dark={dark} {name}: {r:.2}");
                assert!(dh >= 20.0, "dark={dark} {name}: tinta vicina ({dh:.0})");
            }
        }
    }

    #[test]
    fn contrast_ratio_extremes() {
        assert!((contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([10, 20, 30], [10, 20, 30]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn palette_contrast_wcag_aa() {
        let cases = [
            ("testo chiaro", TEXT_LIGHT, BG_LIGHT, 4.5),
            ("testo scuro", TEXT_DARK, BG_DARK, 4.5),
            ("etichetta su pulsante", BG_LIGHT, PRIMARY_STRONG, 4.5),
            ("link chiaro", PRIMARY_STRONG, BG_LIGHT, 4.5),
            ("primario scuro", PRIMARY_DARK, BG_DARK, 3.0),
            ("selezione scura", BG_DARK, PRIMARY_DARK, 4.5),
        ];
        for (name, fg, bg, min) in cases {
            let r = contrast_ratio(rgb(fg), rgb(bg));
            println!("{name}: {r:.2}");
            assert!(r >= min, "{name}: {r:.2} < {min}");
        }
        // Il primario ufficiale con etichetta chiara non raggiunge AA (documentato).
        let raw = contrast_ratio(rgb(BG_LIGHT), rgb(PRIMARY));
        println!("#FAF7F2 su #BE5532: {raw:.2}");
        assert!(raw < 4.5);
    }

    #[test]
    fn faces_scaled_like_svg() {
        let f = faces_at([120.0, 120.0], 0.5);
        // M35 40 → 120 + 0.5*(35-147), 120 + 0.5*(40-120)
        assert_eq!(f[0][0], [64.0, 80.0]);
        assert_eq!(f[3][1], [176.0, 78.5]);
        // tutte le facce stanno nel quadrato 40..200
        for p in f.iter().flatten() {
            assert!((40.0..=200.0).contains(&p[0]) && (40.0..=200.0).contains(&p[1]));
        }
    }

    #[test]
    fn rounded_rect_bounds() {
        let r = rounded_rect(40.0, 40.0, 160.0, 160.0, 36.0, 6);
        let (minx, maxx) = r
            .iter()
            .fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
        assert!((minx - 40.0).abs() < 1e-3 && (maxx - 200.0).abs() < 1e-3);
        assert_eq!(r.len(), 28);
    }

    #[test]
    fn wordmark_inside_viewbox() {
        let s = wordmark_strokes();
        assert_eq!(s.len(), 7);
        for p in s.iter().flatten() {
            assert!(p[0] > 200.0 && p[0] < 803.0 && p[1] > 40.0 && p[1] < 200.0);
        }
        const { assert!(LOGO_RIGHT < 803.0) };
    }

    #[test]
    fn app_icon_buffer() {
        let b = app_icon_rgba();
        assert_eq!(b.len(), 256 * 256 * 4);
        assert_eq!(b[3], 0, "angolo trasparente");
        let o = (128 * 256 + 128) * 4;
        assert_eq!(b[o + 3], 255, "centro opaco");
        // faccia al 80% di #FAF7F2 sopra #BE5532
        let (r, g, bl) = (b[o], b[o + 1], b[o + 2]);
        assert!(
            (0xBE..=0xFA).contains(&r) && g >= 0x55 && bl >= 0x32,
            "{r} {g} {bl}"
        );
        // punto nel quadrato fuori dalle facce: primario pieno
        let o = (20 * 256 + 128) * 4;
        assert_eq!(
            [b[o], b[o + 1], b[o + 2], b[o + 3]],
            [0xBE, 0x55, 0x32, 255]
        );
    }
}
