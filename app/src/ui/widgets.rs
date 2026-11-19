//! Komponen tampilan kecil yang dipakai di semua layar (ramah layar sentuh: tinggi tombol ≥ 44 px).

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

// Token v2 "Periwinkle & Ink" (design/bakadai-lite.figcraft, koleksi variabel "Bakadai v2").
// Nama lama dipertahankan supaya semua layar ikut berubah: PRIMARY = tinta (aksi utama).
pub const PRIMARY: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const PRIMARY_DARK: Color32 = Color32::from_rgb(0x2B, 0x2B, 0x30);
pub const PRIMARY_SOFT: Color32 = Color32::from_rgb(0xE4, 0xEA, 0xFF);
pub const ACCENT: Color32 = Color32::from_rgb(0xB4, 0xC6, 0xFF);
pub const ACCENT_INK: Color32 = Color32::from_rgb(0x24, 0x32, 0x6B);
pub const SURFACE: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xF0);
pub const BG: Color32 = Color32::from_rgb(0xF4, 0xF4, 0xF6);
pub const CARD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(0xE4, 0xE4, 0xE9);
pub const TEXT: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const MUTED: Color32 = Color32::from_rgb(0x66, 0x66, 0x6E);
pub const GREEN: Color32 = Color32::from_rgb(0x1F, 0x7A, 0x45);
pub const GREEN_SOFT: Color32 = Color32::from_rgb(0xDD, 0xF3, 0xE6);
pub const RED: Color32 = Color32::from_rgb(0xB3, 0x26, 0x1E);
pub const RED_SOFT: Color32 = Color32::from_rgb(0xFD, 0xE7, 0xE4);
pub const ORANGE: Color32 = Color32::from_rgb(0x9A, 0x5B, 0x00);
pub const ORANGE_SOFT: Color32 = Color32::from_rgb(0xFF, 0xF1, 0xD6);
/// Dulu ungu; v2 tanpa ungu, jadi status "hampir rendah"/audit memakai tinta periwinkle.
pub const PURPLE: Color32 = ACCENT_INK;

pub const TOUCH_H: f32 = 52.0;

/// Berat Medium untuk judul/tombol (egui `strong()` hanya mengganti warna, bukan berat).
pub fn medium() -> egui::FontFamily {
    egui::FontFamily::Name("medium".into())
}

fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily};
    use std::sync::Arc;
    let mut f = egui::FontDefinitions::default();
    f.font_data.insert("outfit".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Outfit-Regular.ttf"))));
    f.font_data.insert("outfit-medium".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Outfit-Medium.ttf"))));
    let fallback = f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    f.families.entry(FontFamily::Proportional).or_default().insert(0, "outfit".into());
    let mut med = vec!["outfit-medium".to_string()];
    med.extend(fallback); // simbol yang tidak ada di Outfit (mis. ≤) tetap tergambar
    f.families.insert(medium(), med);
    ctx.set_fonts(f);
}

pub fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    install_fonts(ctx);
    ctx.all_styles_mut(|s| {
        use egui::{FontFamily, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(16.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(16.0, medium()));
        s.text_styles.insert(TextStyle::Heading, FontId::new(24.0, medium()));
        s.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
        s.spacing.item_spacing = Vec2::new(10.0, 10.0);
        s.spacing.button_padding = Vec2::new(16.0, 10.0);
        s.spacing.interact_size = Vec2::new(44.0, 44.0);
        s.spacing.text_edit_width = 260.0;
        s.spacing.combo_width = 200.0;
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = BG;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = PRIMARY_SOFT;
        v.selection.stroke = Stroke::new(1.0, PRIMARY);
        v.hyperlink_color = ACCENT_INK;
        v.window_corner_radius = CornerRadius::same(24);
        v.menu_corner_radius = CornerRadius::same(16);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(16);
        }
        v.widgets.inactive.bg_fill = SURFACE;
        v.widgets.inactive.weak_bg_fill = SURFACE;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.hovered.weak_bg_fill = PRIMARY_SOFT;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_INK);
        v.widgets.active.bg_stroke = Stroke::new(1.5, PRIMARY);
        v.window_stroke = Stroke::NONE;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    });
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(24)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).family(medium()).size(18.0));
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED).size(14.0));
}

/// Tombol utama biru selebar layar.
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    let w = ui.available_width();
    ui.add_sized(
        [w, TOUCH_H],
        egui::Button::new(RichText::new(text).color(Color32::WHITE).family(medium())).fill(PRIMARY).corner_radius(26),
    )
}

pub fn primary_button_enabled(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    let w = ui.available_width();
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(if enabled { Color32::WHITE } else { MUTED }).family(medium()))
            .fill(if enabled { PRIMARY } else { SURFACE })
            .corner_radius(26)
            .min_size(Vec2::new(w, TOUCH_H)),
    )
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> egui::Response {
    let w = ui.available_width();
    ui.add_sized(
        [w, TOUCH_H],
        egui::Button::new(RichText::new(text).color(PRIMARY).family(medium()))
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.5, PRIMARY))
            .corner_radius(26),
    )
}

pub fn danger_button(ui: &mut Ui, text: &str) -> egui::Response {
    let w = ui.available_width();
    ui.add_sized(
        [w, TOUCH_H],
        egui::Button::new(RichText::new(text).color(RED).family(medium()))
            .fill(RED_SOFT)
            .corner_radius(26),
    )
}

pub fn small_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(ACCENT_INK).family(medium())).fill(PRIMARY_SOFT).corner_radius(22).min_size(Vec2::new(44.0, 44.0)))
}

/// Label + kolom isian satu baris.
pub fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(RichText::new(label).size(14.0).color(MUTED));
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint.to_string())
            .desired_width(ui.available_width())
            .margin(Margin::symmetric(16, 15)),
    )
}

pub fn field_multiline(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(RichText::new(label).size(14.0).color(MUTED));
    ui.add(
        egui::TextEdit::multiline(value)
            .hint_text(hint.to_string())
            .desired_rows(2)
            .desired_width(ui.available_width())
            .margin(Margin::symmetric(16, 14)),
    )
}

/// Pil warna kecil (status stok, status bayar).
pub fn pill(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(12)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).size(12.0).family(medium()));
        });
}

pub fn pay_pill(ui: &mut Ui, status: &str) {
    match status {
        "Lunas" => pill(ui, "Lunas", GREEN, GREEN_SOFT),
        "Belum Bayar" => pill(ui, "Piutang", RED, RED_SOFT),
        _ => pill(ui, status, ORANGE, ORANGE_SOFT),
    }
}

pub fn stock_pill(ui: &mut Ui, state: crate::db::StockState) {
    use crate::db::StockState::*;
    match state {
        Empty => pill(ui, "Habis", RED, RED_SOFT),
        Low => pill(ui, "Stok Rendah", ORANGE, ORANGE_SOFT),
        NearLow => pill(ui, "Hampir Rendah", ACCENT_INK, PRIMARY_SOFT),
        Ok => {}
    }
}

/// Kotak gambar pengganti (inisial nama di atas warna) untuk barang tanpa foto.
pub fn placeholder_tile(ui: &mut Ui, name: &str, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hue = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let palette = [PRIMARY_SOFT, GREEN_SOFT, ORANGE_SOFT, RED_SOFT, SURFACE];
    let fgs = [ACCENT_INK, GREEN, ORANGE, RED, PRIMARY_DARK];
    let i = (hue % palette.len() as u32) as usize;
    let radius = if size <= 52.0 { size / 2.0 } else { 20.0 };
    ui.painter().rect_filled(rect, radius, palette[i]);
    let initials: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect();
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initials, FontId::new(size * 0.32, medium()), fgs[i]);
    resp
}

/// Ikon digambar sendiri (font bawaan tidak punya simbol), ringan dan tajam di semua kepadatan layar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Home,
    Pos,
    Box,
    History,
    Menu,
    Back,
    Next,
    User,
    Chart,
    Sliders,
    Sync,
    Plus,
    Filter,
    In,
    Out,
    Check,
    Close,
    Dot,
    Scan,
}

pub fn paint_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, s: f32, col: Color32) {
    use egui::{pos2, vec2, Rect};
    let st = Stroke::new((s * 0.11).max(1.5), col);
    let h = s / 2.0;
    match icon {
        Icon::Home => {
            p.line_segment([pos2(c.x - h, c.y - 0.05 * s), pos2(c.x, c.y - h)], st);
            p.line_segment([pos2(c.x, c.y - h), pos2(c.x + h, c.y - 0.05 * s)], st);
            let r = Rect::from_min_max(pos2(c.x - h * 0.7, c.y - 0.1 * s), pos2(c.x + h * 0.7, c.y + h));
            p.rect_stroke(r, 2, st, StrokeKind::Middle);
        }
        Icon::Pos => {
            let r = Rect::from_center_size(c, vec2(s * 0.9, s * 0.8));
            p.rect_stroke(r, 3, st, StrokeKind::Middle);
            p.line_segment([pos2(r.left(), r.top() + s * 0.3), pos2(r.right(), r.top() + s * 0.3)], st);
            for i in 0..2 {
                for j in 0..3 {
                    let q = pos2(r.left() + s * (0.2 + 0.25 * j as f32), r.top() + s * (0.45 + 0.18 * i as f32));
                    p.circle_filled(q, s * 0.05, col);
                }
            }
        }
        Icon::Box => {
            let r = Rect::from_center_size(c + vec2(0.0, s * 0.05), vec2(s * 0.85, s * 0.7));
            p.rect_stroke(r, 2, st, StrokeKind::Middle);
            p.line_segment([pos2(r.left(), r.top() + s * 0.22), pos2(r.right(), r.top() + s * 0.22)], st);
            p.line_segment([pos2(c.x - s * 0.1, r.top() + s * 0.4), pos2(c.x + s * 0.1, r.top() + s * 0.4)], st);
        }
        Icon::History => {
            p.circle_stroke(c, h * 0.9, st);
            p.line_segment([c, pos2(c.x, c.y - h * 0.55)], st);
            p.line_segment([c, pos2(c.x + h * 0.4, c.y + h * 0.2)], st);
        }
        Icon::Menu => {
            for k in [-1.0, 0.0, 1.0] {
                let y = c.y + k * s * 0.28;
                p.line_segment([pos2(c.x - h * 0.85, y), pos2(c.x + h * 0.85, y)], st);
            }
        }
        Icon::Back => {
            p.line_segment([pos2(c.x + h * 0.3, c.y - h * 0.7), pos2(c.x - h * 0.4, c.y)], st);
            p.line_segment([pos2(c.x - h * 0.4, c.y), pos2(c.x + h * 0.3, c.y + h * 0.7)], st);
        }
        Icon::Next => {
            p.line_segment([pos2(c.x - h * 0.3, c.y - h * 0.7), pos2(c.x + h * 0.4, c.y)], st);
            p.line_segment([pos2(c.x + h * 0.4, c.y), pos2(c.x - h * 0.3, c.y + h * 0.7)], st);
        }
        Icon::User => {
            p.circle_stroke(pos2(c.x, c.y - h * 0.35), h * 0.38, st);
            let r = Rect::from_min_max(pos2(c.x - h * 0.75, c.y + h * 0.25), pos2(c.x + h * 0.75, c.y + h * 1.25));
            p.rect_stroke(r, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, st, StrokeKind::Middle);
        }
        Icon::Chart => {
            for (k, top) in [(-0.6f32, 0.1f32), (0.0, -0.7), (0.6, -0.2)] {
                p.line_segment([pos2(c.x + k * h, c.y + h * 0.8), pos2(c.x + k * h, c.y + top * h)], st);
            }
            p.line_segment([pos2(c.x - h, c.y + h * 0.8), pos2(c.x + h, c.y + h * 0.8)], st);
        }
        Icon::Sliders => {
            for (y, kx) in [(-0.45f32, 0.35f32), (0.45, -0.35)] {
                p.line_segment([pos2(c.x - h * 0.85, c.y + y * h), pos2(c.x + h * 0.85, c.y + y * h)], st);
                p.circle_filled(pos2(c.x + kx * h, c.y + y * h), s * 0.11, col);
            }
        }
        Icon::Sync => {
            let r = h * 0.75;
            for a in [0.3f32, 3.44] {
                let pts: Vec<egui::Pos2> = (0..=10).map(|i| {
                    let t = a + i as f32 * 0.24;
                    pos2(c.x + r * t.cos(), c.y + r * t.sin())
                }).collect();
                p.add(egui::Shape::line(pts, st));
            }
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - h * 0.7, c.y), pos2(c.x + h * 0.7, c.y)], st);
            p.line_segment([pos2(c.x, c.y - h * 0.7), pos2(c.x, c.y + h * 0.7)], st);
        }
        Icon::Filter => {
            for (k, w) in [(-1.0, 0.85), (0.0, 0.55), (1.0, 0.25)] {
                let y = c.y + k * s * 0.25;
                p.line_segment([pos2(c.x - h * w, y), pos2(c.x + h * w, y)], st);
            }
        }
        Icon::In | Icon::Out => {
            let (a, b) = if icon == Icon::In { (c.y - h * 0.8, c.y + h * 0.6) } else { (c.y + h * 0.8, c.y - h * 0.6) };
            p.line_segment([pos2(c.x, a), pos2(c.x, b)], st);
            let dir = if icon == Icon::In { -1.0 } else { 1.0 };
            p.line_segment([pos2(c.x - h * 0.5, b + dir * h * 0.5), pos2(c.x, b)], st);
            p.line_segment([pos2(c.x + h * 0.5, b + dir * h * 0.5), pos2(c.x, b)], st);
        }
        Icon::Check => {
            p.line_segment([pos2(c.x - h * 0.7, c.y), pos2(c.x - h * 0.15, c.y + h * 0.55)], st);
            p.line_segment([pos2(c.x - h * 0.15, c.y + h * 0.55), pos2(c.x + h * 0.75, c.y - h * 0.6)], st);
        }
        Icon::Close => {
            p.line_segment([pos2(c.x - h * 0.6, c.y - h * 0.6), pos2(c.x + h * 0.6, c.y + h * 0.6)], st);
            p.line_segment([pos2(c.x + h * 0.6, c.y - h * 0.6), pos2(c.x - h * 0.6, c.y + h * 0.6)], st);
        }
        Icon::Dot => {
            p.circle_filled(c, h * 0.5, col);
        }
        Icon::Scan => {
            // Empat sudut bidik + garis tengah.
            let (a, k) = (h * 0.85, h * 0.4);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let q = pos2(c.x + sx * a, c.y + sy * a);
                p.line_segment([q, pos2(q.x - sx * k, q.y)], st);
                p.line_segment([q, pos2(q.x, q.y - sy * k)], st);
            }
            p.line_segment([pos2(c.x - a * 0.7, c.y), pos2(c.x + a * 0.7, c.y)], st);
        }
    }
}

/// Tombol ikon kecil (tanpa teks).
pub fn icon_button(ui: &mut Ui, icon: Icon, col: Color32, fill: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(44.0, 44.0), Sense::click());
    if fill != Color32::TRANSPARENT {
        ui.painter().circle_filled(rect.center(), 22.0, if resp.hovered() { fill.gamma_multiply(0.85) } else { fill });
    }
    paint_icon(ui.painter(), icon, rect.center(), 20.0, col);
    resp
}

/// Potong teks dengan "…" agar muat satu baris.
pub fn fit_text(p: &egui::Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let w = |s: &str| p.layout_no_wrap(s.to_string(), font.clone(), TEXT).size().x;
    if w(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let s: String = chars[..n].iter().collect::<String>() + "…";
        if w(&s) <= max_w {
            return s;
        }
    }
    "…".into()
}

/// Kartu angka ringkas (beranda/laporan).
pub fn stat_tile(ui: &mut Ui, width: f32, label: &str, value: &str, color: Color32) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(24)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            ui.set_max_width(width - 32.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let lf = FontId::proportional(14.0);
                let vf = FontId::new(18.0, medium());
                let max = width - 32.0;
                let l = fit_text(ui.painter(), label, &lf, max);
                let v = fit_text(ui.painter(), value, &vf, max);
                ui.label(RichText::new(l).font(lf).color(MUTED));
                ui.label(RichText::new(v).font(vf).color(color));
            });
        });
}

/// Baris "label ........ nilai".
pub fn kv(ui: &mut Ui, k: &str, v: &str) {
    kv_color(ui, k, v, TEXT);
}

pub fn kv_color(ui: &mut Ui, k: &str, v: &str, c: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
    let p = ui.painter();
    let vr = p.text(rect.right_center(), Align2::RIGHT_CENTER, v, FontId::new(16.0, medium()), c);
    let kf = FontId::proportional(15.0);
    let k = fit_text(p, k, &kf, (vr.left() - rect.left() - 10.0).max(30.0));
    p.text(rect.left_center(), Align2::LEFT_CENTER, k, kf, MUTED);
}

/// Baris kolom isian + tombol di kanan, tanpa melebar keluar layar.
pub fn input_with_button(ui: &mut Ui, value: &mut String, hint: &str, button: &str, char_limit: usize) -> bool {
    let mut clicked = false;
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 52.0), Layout::right_to_left(Align::Center), |ui| {
        clicked = small_button(ui, button).clicked();
        let mut te = egui::TextEdit::singleline(value).hint_text(hint.to_string()).desired_width(ui.available_width()).margin(Margin::symmetric(16, 15));
        if char_limit > 0 {
            te = te.char_limit(char_limit);
        }
        ui.add(te);
    });
    clicked
}

/// Tombol aksi cepat berbentuk ikon bulat + label (beranda).
pub fn quick_action(ui: &mut Ui, width: f32, icon: Icon, label: &str, color: Color32) -> egui::Response {
    let _ = color; // v2: semua aksi memakai lingkaran tinta yang sama
    let h = 92.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let p = ui.painter();
    let c = egui::pos2(rect.center().x, rect.top() + 30.0);
    p.circle_filled(c, 30.0, if resp.hovered() { PRIMARY_DARK } else { PRIMARY });
    paint_icon(p, icon, c, 22.0, Color32::WHITE);
    p.text(egui::pos2(rect.center().x, rect.bottom() - 12.0), Align2::CENTER_CENTER, label, FontId::proportional(14.0), TEXT);
    resp
}

/// Garis grafik batang sederhana (penjualan harian), digambar langsung tanpa pustaka grafik.
pub fn bar_chart(ui: &mut Ui, data: &[(String, i64)], color: Color32) {
    let h = 72.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h + 18.0), Sense::hover());
    let p = ui.painter();
    let max = data.iter().map(|d| d.1).max().unwrap_or(0).max(1) as f32;
    let n = data.len().max(1) as f32;
    let slot = rect.width() / n;
    let bw = (slot * 0.3).clamp(2.0, 10.0);
    for (i, (label, v)) in data.iter().enumerate() {
        let x = rect.left() + slot * i as f32 + (slot - bw) / 2.0;
        let bh = (*v as f32 / max * h).max(if *v > 0 { 2.0 } else { 0.0 });
        let r = egui::Rect::from_min_max(egui::pos2(x, rect.top() + h - bh), egui::pos2(x + bw, rect.top() + h));
        p.rect_filled(r, (bw / 2.0).min(6.0), color);
        let every = (data.len() / 7).max(1);
        if i % every == 0 || i + 1 == data.len() {
            p.text(egui::pos2(x + bw / 2.0, rect.top() + h + 9.0), Align2::CENTER_CENTER, label, FontId::proportional(10.0), MUTED);
        }
    }
    p.line_segment([egui::pos2(rect.left(), rect.top() + h), egui::pos2(rect.right(), rect.top() + h)], Stroke::new(1.0, BORDER));
}

/// Angka uang: titik ribuan diabaikan ("12.500" → 12500), koma = desimal.
pub fn parse_money(s: &str) -> f64 {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '-').collect();
    t.replace(',', ".").parse().unwrap_or(0.0)
}

/// Jumlah barang: titik atau koma sebagai desimal ("1,5" / "1.5" → 1.5).
pub fn parse_qty(s: &str) -> f64 {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.' || *c == '-').collect();
    t.replace(',', ".").parse().unwrap_or(0.0)
}

pub fn money_str(m: crate::db::Money) -> String {
    let v = crate::db::money_to_f64(m);
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.2}", v).replace('.', ",")
    }
}

pub fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
        paint_icon(ui.painter(), Icon::Box, r.center(), 36.0, BORDER);
        ui.label(RichText::new(title).family(medium()).size(16.0));
        ui.label(RichText::new(hint).color(MUTED));
    });
    ui.add_space(20.0);
}

/// Pilihan segmen (mis. Persen / Nominal).
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (v, label) in options {
            let on = *value == *v;
            let b = egui::Button::new(RichText::new(*label).color(if on { Color32::WHITE } else { TEXT }).size(14.0))
                .fill(if on { PRIMARY } else { CARD })
                .stroke(if on { Stroke::NONE } else { Stroke::new(1.0, BORDER) })
                .corner_radius(22)
                .min_size(Vec2::new(0.0, 44.0));
            if ui.add(b).clicked() && !on {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

/// Panel putih tempat beberapa baris menyatu (v2: satu kartu, bukan kartu per baris).
pub fn list_group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(24)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// Saklar (pengganti checkbox): label kiri, pil 52×32 kanan, seluruh baris bisa diketuk.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: impl Into<String>) -> egui::Response {
    let label: String = label.into();
    let w = ui.available_width();
    let col = if ui.is_enabled() { TEXT } else { MUTED };
    let galley = ui.painter().layout(label.clone(), FontId::proportional(16.0), col, (w - 68.0).max(60.0));
    let h = (galley.size().y + 20.0).max(52.0);
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let p = ui.painter();
    p.galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, col);
    let sw = egui::Rect::from_min_size(egui::pos2(rect.right() - 52.0, rect.center().y - 16.0), Vec2::new(52.0, 32.0));
    p.rect_filled(sw, 16, if *on { PRIMARY } else { SURFACE });
    p.circle_filled(egui::pos2(egui::lerp((sw.left() + 16.0)..=(sw.right() - 16.0), t), sw.center().y), 12.0, CARD);
    let on_now = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, on_now, &label));
    resp
}

/// Pilihan berbentuk pil (pengganti selectable_label): aktif = tinta, lain = putih bergaris.
pub fn chip(ui: &mut Ui, on: bool, label: impl Into<String>) -> egui::Response {
    let label: String = label.into();
    ui.add(
        egui::Button::new(RichText::new(label).size(14.0).color(if on { Color32::WHITE } else { TEXT }))
            .fill(if on { PRIMARY } else { CARD })
            .stroke(if on { Stroke::NONE } else { Stroke::new(1.0, BORDER) })
            .corner_radius(22)
            .min_size(Vec2::new(44.0, 44.0)),
    )
}

/// Ikon bulat per jenis transaksi (riwayat, beranda).
pub fn kind_icon(k: crate::db::TxKind) -> (Icon, Color32, Color32) {
    use crate::db::TxKind::*;
    match k {
        Out => (Icon::Pos, Color32::WHITE, PRIMARY),
        In => (Icon::In, GREEN, GREEN_SOFT),
        SaleReturn | PurchaseReturn => (Icon::Back, ORANGE, ORANGE_SOFT),
        Audit => (Icon::Check, ACCENT_INK, PRIMARY_SOFT),
    }
}

/// Baris daftar yang bisa diketuk (judul, sub-judul, nilai kanan).
pub fn list_row(ui: &mut Ui, title: &str, subtitle: &str, right: &str, right_color: Color32) -> egui::Response {
    list_row_icon(ui, None, title, subtitle, right, right_color)
}

/// Seperti list_row, dengan ikon bulat (ikon, warna ikon, latar) di kiri.
pub fn list_row_icon(ui: &mut Ui, icon: Option<(Icon, Color32, Color32)>, title: &str, subtitle: &str, right: &str, right_color: Color32) -> egui::Response {
    let w = ui.available_width();
    let h = if subtitle.is_empty() { 56.0 } else { 68.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 20, if resp.hovered() { PRIMARY_SOFT } else { CARD });
    let mut left = rect.left() + 16.0;
    if let Some((ic, fg, bg)) = icon {
        let c = egui::pos2(left + 20.0, rect.center().y);
        p.circle_filled(c, 20.0, bg);
        paint_icon(p, ic, c, 18.0, fg);
        left += 52.0;
    }
    let right_rect = p.text(egui::pos2(rect.right() - 16.0, rect.center().y), Align2::RIGHT_CENTER, right, FontId::new(15.0, medium()), right_color);
    let max_w = (right_rect.left() - left - 10.0).max(40.0);
    let tf = FontId::new(16.0, medium());
    let title = fit_text(p, title, &tf, max_w);
    if subtitle.is_empty() {
        p.text(egui::pos2(left, rect.center().y), Align2::LEFT_CENTER, title, tf, TEXT);
    } else {
        p.text(egui::pos2(left, rect.top() + 13.0), Align2::LEFT_TOP, title, tf, TEXT);
        let sf = FontId::proportional(13.0);
        let sub = fit_text(p, subtitle, &sf, max_w);
        p.text(egui::pos2(left, rect.top() + 38.0), Align2::LEFT_TOP, sub, sf, MUTED);
    }
    resp
}
