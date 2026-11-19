//! Tampilan utama: navigasi bawah ala aplikasi HP (Beranda, Kasir, Barang, Riwayat, Lainnya).

mod more;
mod pos;
mod products;
mod stock;
pub mod widgets;

use crate::db::{self, Db, Money, TxKind};
use crate::sync::{SyncConfig, SyncResult};
use egui::{Align, Color32, Layout, Margin, RichText, Ui, Vec2};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use widgets::*;

#[derive(Clone, PartialEq, Debug)]
pub enum Screen {
    Home,
    Pos,
    Products,
    ProductEdit,
    ProductDetail(String),
    StockForm(TxKind),
    History,
    TxDetail(String),
    Return(String),
    More,
    Contacts(&'static str),
    ContactEdit,
    Receivables,
    Expenses,
    ExpenseEdit,
    Staff,
    StaffEdit,
    Reports,
    Settings,
    Categories(&'static str),
    PaymentMethods,
    Fees,
    Sync,
}

impl Screen {
    fn title(&self) -> &'static str {
        match self {
            Screen::Home => "Beranda",
            Screen::Pos => "Kasir",
            Screen::Products => "Barang",
            Screen::ProductEdit => "Barang",
            Screen::ProductDetail(_) => "Detail Barang",
            Screen::StockForm(k) => k.label(),
            Screen::History => "Riwayat",
            Screen::TxDetail(_) => "Detail Transaksi",
            Screen::Return(_) => "Buat Retur",
            Screen::More => "Lainnya",
            Screen::Contacts(k) => {
                if *k == "vendor" {
                    "Vendor"
                } else {
                    "Pembeli"
                }
            }
            Screen::ContactEdit => "Kontak",
            Screen::Receivables => "Laporan Piutang",
            Screen::Expenses => "Pengeluaran",
            Screen::ExpenseEdit => "Pengeluaran",
            Screen::Staff => "Staff",
            Screen::StaffEdit => "Staff",
            Screen::Reports => "Laporan",
            Screen::Settings => "Pengaturan",
            Screen::Categories(_) => "Kategori",
            Screen::PaymentMethods => "Metode Pembayaran",
            Screen::Fees => "Biaya Tambahan",
            Screen::Sync => "Server & Sinkron",
        }
    }
}

/// Batas tekstur thumbnail di memori (LRU) — menjaga RAM tetap kecil walau barang ribuan.
const THUMB_CACHE_MAX: usize = 96;
/// Maksimal thumbnail yang dibaca dari disk per frame, supaya gulir tetap mulus.
const THUMB_LOADS_PER_FRAME: usize = 4;

pub struct ThumbCache {
    map: HashMap<String, Option<egui::TextureHandle>>,
    order: VecDeque<String>,
    loads_this_frame: usize,
}

impl ThumbCache {
    fn new() -> Self {
        ThumbCache { map: HashMap::new(), order: VecDeque::new(), loads_this_frame: 0 }
    }
    pub fn get(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.map.get(path) {
            return t.clone();
        }
        if self.loads_this_frame >= THUMB_LOADS_PER_FRAME {
            ctx.request_repaint();
            return None;
        }
        self.loads_this_frame += 1;
        let tex = crate::imgproc::load_thumb(Path::new(path)).map(|img| ctx.load_texture(path, img, egui::TextureOptions::LINEAR));
        self.map.insert(path.to_string(), tex.clone());
        self.order.push_back(path.to_string());
        while self.order.len() > THUMB_CACHE_MAX {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        tex
    }
    pub fn forget(&mut self, path: &str) {
        self.map.remove(path);
    }
}

struct ShotState {
    path: PathBuf,
    frame: u32,
    requested: bool,
    done: Arc<AtomicBool>,
}

struct ScanUi {
    cam: crate::scan::Camera,
    tex: Option<egui::TextureHandle>,
    /// Bingkai ke thread dekoder (kapasitas 1: bingkai dilewati selama dekoder masih sibuk).
    tx: SyncSender<(Vec<u8>, u32, u32)>,
    rx: Receiver<String>,
}

pub struct BakadaiApp {
    pub db: Db,
    pub data_dir: PathBuf,
    pub nav: Vec<Screen>,
    pub rev: u64,
    pub toast: Option<(String, f64, bool)>,
    pub thumbs: ThumbCache,
    pub currency: String,
    pub staff_id: String,
    pub perms: Vec<String>,
    pub business_name: String,
    shot: Option<ShotState>,
    pub sync_rx: Option<Receiver<SyncResult>>,
    print_rx: Option<Receiver<Result<(), String>>>,
    pub sync_status: String,
    pub confirm: Option<(String, Confirm)>,
    pub home: more::HomeState,
    pub products: products::ProductsState,
    pub pos: pos::PosState,
    pub stock: stock::StockState,
    pub more: more::MoreState,
    /// Safe area (titik egui): tinggi bar status & bar navigasi Android.
    insets: (f32, f32),
    insets_for: Option<Vec2>,
    /// Layar scan kamera yang sedang terbuka.
    scan: Option<ScanUi>,
    /// Kode hasil scan kamera, diambil layar yang membuka scanner lewat [`BakadaiApp::take_scan`].
    scanned: Option<String>,
    /// Permintaan buka scanner dari layar yang sedang meminjam state-nya (dibuka di awal bingkai berikutnya).
    pub want_scan: bool,
    /// Waktu (detik egui) tombol kembali ditekan di layar terakhir; tekan lagi dalam 2 detik = keluar.
    exit_at: f64,
}

#[derive(Clone, Debug)]
pub enum Confirm {
    DeleteProduct(String),
    DeleteTx(String),
    DeleteContact(String),
    DeleteExpense(String),
    DeleteStaff(String),
    DeleteCategory(String),
    ClearCart,
}

impl BakadaiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, data_dir: PathBuf, screenshot: Option<(PathBuf, String)>) -> Self {
        apply_theme(&cc.egui_ctx);
        let mut db = match Db::open(&data_dir) {
            Ok(d) => d,
            Err(e) => {
                // Folder data rusak/tidak bisa dibuat: pakai folder sementara agar aplikasi tetap terbuka.
                let tmp = std::env::temp_dir().join("bakadai-lite-fallback");
                eprintln!("Tidak bisa membuka data di {:?}: {}", data_dir, e);
                Db::open(&tmp).expect("database sementara")
            }
        };
        if db.setting("seeded", "0") == "0" {
            let _ = db.seed_demo();
            let _ = db.set_setting("seeded", "1");
        }
        // Versi uji lama mengisi mata uang dari negara HP; sekali saja kembalikan ke Rupiah.
        if db.setting("currency_reset", "0") == "0" {
            let _ = db.set_setting("currency", "Rp");
            let _ = db.set_setting("currency_reset", "1");
        }
        if db.setting("device_id", "").is_empty() {
            let _ = db.set_setting("device_id", &format!("dev-{}", db::new_id()));
        }
        let staff = db.staff();
        let saved = db.setting("current_staff", "");
        let me = staff.iter().find(|s| s.id == saved).or_else(|| staff.first()).cloned().unwrap_or_default();
        let start = screenshot
            .as_ref()
            .map(|(_, s)| match s.as_str() {
                "pos" | "kasir" => Screen::Pos,
                "products" | "barang" => Screen::Products,
                "history" | "riwayat" => Screen::History,
                "more" | "lainnya" => Screen::More,
                "reports" | "laporan" => Screen::Reports,
                "staff" => Screen::Staff,
                "stockin" => Screen::StockForm(TxKind::In),
                "settings" => Screen::Settings,
                "edit" => Screen::ProductEdit,
                "sync" => Screen::Sync,
                _ => Screen::Home,
            })
            .unwrap_or(Screen::Home);
        let mut app = BakadaiApp {
            currency: db.setting("currency", "Rp"),
            business_name: db.setting("business_name", "Toko Saya"),
            staff_id: me.id.clone(),
            perms: me.permissions.clone(),
            db,
            data_dir,
            nav: vec![start.clone()],
            rev: 1,
            toast: None,
            thumbs: ThumbCache::new(),
            shot: screenshot.map(|(p, _)| ShotState { path: p, frame: 0, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            sync_rx: None,
            print_rx: None,
            sync_status: String::new(),
            confirm: None,
            home: Default::default(),
            products: Default::default(),
            pos: Default::default(),
            stock: Default::default(),
            more: Default::default(),
            insets: (0.0, 0.0),
            insets_for: None,
            exit_at: -10.0,
            scan: None,
            scanned: None,
            want_scan: false,
        };
        if start == Screen::ProductEdit {
            app.products.open_new(&app.db);
        }
        if start == Screen::Pos {
            // Isi keranjang contoh agar gambar cek menampilkan alur kasir.
            for code in ["8990001", "8990004"] {
                if let Some(p) = app.db.product_by_code(code) {
                    app.pos.add_product(&p, if code == "8990004" { 3.0 } else { 1.0 });
                }
            }
        }
        app
    }

    pub fn can(&self, perm: &str) -> bool {
        self.perms.iter().any(|p| p == perm)
    }

    pub fn money(&self, m: Money) -> String {
        db::fmt_money(m, &self.currency)
    }

    /// Buka kamera untuk scan barcode; hasilnya diambil layar yang sama lewat [`Self::take_scan`].
    pub fn open_scanner(&mut self) {
        match crate::scan::Camera::open() {
            Ok(cam) => {
                let (tx, frames) = std::sync::mpsc::sync_channel::<(Vec<u8>, u32, u32)>(1);
                let (found, rx) = std::sync::mpsc::channel();
                // Dekode di thread sendiri supaya pratinjau kamera tidak tersendat; berhenti saat scanner ditutup.
                std::thread::spawn(move || {
                    for (luma, w, h) in frames {
                        if let Some(code) = crate::scan::decode(&luma, w, h) {
                            if found.send(code).is_err() {
                                break;
                            }
                        }
                    }
                });
                self.scan = Some(ScanUi { cam, tex: None, tx, rx });
            }
            Err(e) => self.err(e),
        }
    }
    pub fn take_scan(&mut self) -> Option<String> {
        self.scanned.take()
    }

    fn scan_ui(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.want_scan) {
            self.open_scanner();
        }
        let Some(s) = &mut self.scan else { return };
        ctx.request_repaint();
        if let Some(f) = s.cam.frame() {
            let img = egui::ColorImage::from_gray([f.w, f.h], &f.luma);
            match &mut s.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => s.tex = Some(ctx.load_texture("scan", img, egui::TextureOptions::LINEAR)),
            }
            let _ = s.tx.try_send((f.luma, f.w as u32, f.h as u32));
        }
        let found = s.rx.try_recv().ok();
        let mut close = false;
        egui::Modal::new(egui::Id::unique("scan")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.label(RichText::new("Scan barcode").family(medium()).size(17.0));
            ui.label(RichText::new("Arahkan kamera ke barcode barang").color(MUTED));
            ui.add_space(6.0);
            match &s.tex {
                Some(t) => {
                    let r = ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::new(300.0, 400.0)).corner_radius(16)).rect;
                    ui.painter().line_segment([egui::pos2(r.left() + 24.0, r.center().y), egui::pos2(r.right() - 24.0, r.center().y)], egui::Stroke::new(2.0, RED));
                }
                None => {
                    ui.allocate_ui(Vec2::new(300.0, 400.0), |ui| ui.centered_and_justified(|ui| ui.spinner()));
                }
            }
            ui.add_space(8.0);
            if secondary_button(ui, "Batal").clicked() {
                close = true;
            }
        });
        if let Some(code) = found {
            self.scanned = Some(code);
            close = true;
        }
        if close {
            self.scan = None; // menutup kamera; thread dekoder ikut berhenti
            ctx.request_repaint();
        }
    }

    pub fn ok(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 2.5, false));
    }
    pub fn err(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 4.0, true));
    }

    /// Dipanggil setelah setiap perubahan data: semua daftar akan dimuat ulang sekali.
    pub fn changed(&mut self) {
        self.rev += 1;
    }

    pub fn go(&mut self, s: Screen) {
        self.nav.push(s);
    }
    pub fn back(&mut self) {
        if self.nav.len() > 1 {
            self.nav.pop();
        }
    }
    pub fn tab(&mut self, s: Screen) {
        self.nav = vec![s];
    }
    pub fn current(&self) -> Screen {
        self.nav.last().cloned().unwrap_or(Screen::Home)
    }

    pub fn sync_config(&self) -> SyncConfig {
        SyncConfig {
            base_url: self.db.setting("api_base_url", ""),
            token: self.db.setting("api_token", ""),
            device_id: self.db.setting("device_id", ""),
        }
    }

    pub fn switch_staff(&mut self, id: &str) {
        if let Some(s) = self.db.staff().into_iter().find(|s| s.id == id) {
            self.staff_id = s.id.clone();
            self.perms = s.permissions.clone();
            let _ = self.db.set_setting("current_staff", &s.id);
            self.ok(format!("Masuk sebagai {}", s.name));
        }
    }

    /// Cetak struk di thread latar supaya layar tidak tertahan saat menyambung ke printer.
    pub fn print_receipt(&mut self, ctx: &egui::Context, text: &str) {
        let addr = self.db.setting("printer_addr", "");
        if addr.is_empty() {
            return self.err("Isi alamat printer di Pengaturan > Printer");
        }
        if self.print_rx.is_some() {
            return self.err("Masih mencetak struk sebelumnya");
        }
        let bytes = crate::escpos::to_escpos(text, self.db.setting_bool("open_drawer", false));
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::escpos::print(&addr, &bytes));
            ctx.request_repaint();
        });
        self.print_rx = Some(rx);
        self.ok("Mencetak struk…");
    }

    fn poll_print(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let r = match self.print_rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(r)) => r,
            Some(Err(TryRecvError::Disconnected)) => Err("Pencetakan berhenti tiba-tiba".into()),
            _ => return,
        };
        self.print_rx = None;
        match r {
            Ok(_) => self.ok("Struk dikirim ke printer"),
            Err(e) => self.err(e),
        }
    }

    fn poll_sync(&mut self) {
        if let Some(rx) = &self.sync_rx {
            if let Ok(r) = rx.try_recv() {
                self.sync_rx = None;
                match r {
                    SyncResult::Ok { pushed, pulled, photos } => {
                        self.sync_status = format!("Sinkron berhasil: {} dikirim, {} diterima, {} foto", pushed, pulled, photos);
                        self.ok("Sinkron berhasil");
                        self.changed();
                    }
                    SyncResult::LoggedIn { token, name } => {
                        let _ = self.db.set_setting("api_token", &token);
                        self.sync_status = format!("Berhasil masuk{}", if name.is_empty() { String::new() } else { format!(" sebagai {}", name) });
                        self.ok("Sign In Berhasil");
                    }
                    SyncResult::Health(s) => {
                        self.sync_status = s.clone();
                        self.ok(s);
                    }
                    SyncResult::Err(e) => {
                        self.sync_status = e.clone();
                        self.err(e);
                    }
                }
            }
        }
    }

    fn run_confirm(&mut self, c: Confirm) {
        let r = match &c {
            Confirm::DeleteProduct(id) => self.db.delete_products(&[id.clone()]).map(|_| "Barang berhasil dihapus"),
            Confirm::DeleteTx(id) => self.db.delete_tx(id).map(|_| "Riwayat berhasil dihapus"),
            Confirm::DeleteContact(id) => self.db.delete_contact(id).map(|_| "Kontak berhasil dihapus"),
            Confirm::DeleteExpense(id) => self.db.delete_expense(id).map(|_| "Pengeluaran berhasil dihapus"),
            Confirm::DeleteStaff(id) => self.db.delete_staff(id).map(|_| "Staff berhasil terhapus"),
            Confirm::DeleteCategory(id) => self.db.delete_category(id).map(|_| "Kategori berhasil dihapus"),
            Confirm::ClearCart => {
                self.pos.clear();
                Ok("Keranjang dikosongkan")
            }
        };
        match r {
            Ok(m) => {
                self.ok(m);
                self.changed();
                if matches!(c, Confirm::DeleteProduct(_) | Confirm::DeleteTx(_) | Confirm::DeleteContact(_) | Confirm::DeleteStaff(_)) {
                    self.back();
                }
            }
            Err(e) => self.err(e),
        }
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        let screen = self.current();
        let can_back = self.nav.len() > 1;
        // Beranda: menyatu dengan pita periwinkle di bawahnya; layar lain: judul besar di atas latar terang.
        let home = screen == Screen::Home && !can_back;
        egui::Panel::top("top")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(if home { ACCENT } else { BG }).inner_margin(Margin { left: 16, right: 16, top: (14.0 + self.insets.0).min(127.0) as i8, bottom: 10 }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(44.0);
                    if can_back && icon_button(ui, Icon::Back, TEXT, CARD).clicked() {
                        self.back();
                    }
                    let title = if screen == Screen::Home { self.business_name.clone() } else { screen.title().to_string() };
                    ui.label(RichText::new(title).size(if can_back { 20.0 } else { 24.0 }).family(medium()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let pending = self.db.outbox_count();
                        let (txt, col) = if self.sync_rx.is_some() {
                            ("sinkron…".to_string(), TEXT)
                        } else if pending > 0 {
                            (format!("{} antre", pending), ORANGE)
                        } else {
                            ("tersimpan".to_string(), GREEN)
                        };
                        let r = ui.add(
                            egui::Button::new(RichText::new(format!("    {}", txt)).size(13.0).color(col))
                                .fill(CARD)
                                .corner_radius(22)
                                .min_size(Vec2::new(44.0, 36.0)),
                        );
                        paint_icon(ui.painter(), Icon::Dot, r.rect.left_center() + Vec2::new(16.0, 0.0), 10.0, col);
                        if r.clicked() {
                            self.go(Screen::Sync);
                        }
                    });
                });
            });
    }

    fn bottom_bar(&mut self, ui: &mut Ui) {
        let root = self.nav.first().cloned().unwrap_or(Screen::Home);
        let tabs: Vec<(Screen, Icon, &str)> = [
            (Screen::Home, Icon::Home, "Beranda", ""),
            (Screen::Pos, Icon::Pos, "Kasir", "pos"),
            (Screen::Products, Icon::Box, "Barang", ""),
            (Screen::History, Icon::History, "Riwayat", ""),
            (Screen::More, Icon::Menu, "Lainnya", ""),
        ]
        .into_iter()
        .filter(|(_, _, _, perm)| perm.is_empty() || self.can(perm))
        .map(|(s, g, l, _)| (s, g, l))
        .collect();
        // Pil tinta melayang; tab aktif = pil periwinkle berlabel, tab lain ikon saja (label lewat hover/aksesibilitas).
        egui::Panel::bottom("nav")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 16, right: 16, top: 6, bottom: (16.0 + self.insets.1).min(127.0) as i8 }))
            .show(ui, |ui| {
                let w = ui.available_width();
                let (bar, _) = ui.allocate_exact_size(Vec2::new(w, 68.0), egui::Sense::hover());
                let p = ui.painter().clone();
                p.rect_filled(bar, 34, PRIMARY);
                let n = tabs.len().max(1) as f32;
                let active_w = 124.0f32.min(bar.width() * 0.4);
                let other_w = (bar.width() - 16.0 - active_w) / (n - 1.0).max(1.0);
                let mut x = bar.left() + 8.0;
                for (s, glyph, label) in tabs {
                    let on = root == s;
                    let iw = if on { active_w } else { other_w };
                    let r = egui::Rect::from_min_size(egui::pos2(x, bar.top() + 8.0), Vec2::new(iw, 52.0));
                    x += iw;
                    let resp = ui.interact(r, ui.scope_id().with(label), egui::Sense::click()).on_hover_text(label);
                    if on {
                        p.rect_filled(r, 26, ACCENT);
                        let tf = egui::FontId::new(16.0, medium());
                        let tw = p.layout_no_wrap(label.to_string(), tf.clone(), TEXT).size().x;
                        let start = r.center().x - (20.0 + 8.0 + tw) / 2.0;
                        paint_icon(&p, glyph, egui::pos2(start + 10.0, r.center().y), 20.0, TEXT);
                        p.text(egui::pos2(start + 28.0, r.center().y), egui::Align2::LEFT_CENTER, label, tf, TEXT);
                    } else {
                        if resp.hovered() {
                            p.circle_filled(r.center(), 24.0, PRIMARY_DARK);
                        }
                        paint_icon(&p, glyph, r.center(), 20.0, Color32::WHITE);
                    }
                    if resp.clicked() {
                        self.tab(s);
                    }
                }
            });
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        if let Some((msg, left, is_err)) = &mut self.toast {
            *left -= dt;
            let (m, e) = (msg.clone(), *is_err);
            if *left <= 0.0 {
                self.toast = None;
            } else {
                ctx.request_repaint();
                egui::Area::new(egui::Id::unique("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -104.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(if e { RED } else { PRIMARY })
                            .corner_radius(24)
                            .inner_margin(Margin::symmetric(18, 12))
                            .show(ui, |ui| {
                                ui.set_max_width(320.0);
                                ui.label(RichText::new(m).color(Color32::WHITE));
                            });
                    });
            }
        }
    }

    fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, c)) = self.confirm.clone() else { return };
        let mut close = false;
        let mut run = false;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(280.0);
            ui.label(RichText::new("Apakah Anda yakin?").family(medium()).size(17.0));
            ui.label(msg);
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                if secondary_button(&mut cols[0], "Batal").clicked() {
                    close = true;
                }
                if danger_button(&mut cols[1], "Ya").clicked() {
                    run = true;
                }
            });
        });
        if run {
            self.confirm = None;
            self.run_confirm(c);
        } else if close {
            self.confirm = None;
        }
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else { return };
        // Keep drawing even if the window is covered or not focused, otherwise the picture never comes.
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        ctx.request_repaint();
        shot.frame += 1;
        if shot.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if shot.frame >= 120 && !shot.requested {
            shot.requested = true;
            let path = shot.path.clone();
            let done = shot.done.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    bytes.extend_from_slice(&p.to_srgba_unmultiplied());
                }
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    Some(buf) => {
                        if let Err(e) = buf.save(&path) {
                            eprintln!("Gagal menyimpan gambar: {}", e);
                        }
                    }
                    None => eprintln!("Gambar layar tidak valid"),
                }
                done.store(true, Ordering::SeqCst);
            });
        }
    }
}

impl eframe::App for BakadaiApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.shot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.thumbs.loads_this_frame = 0;
        self.poll_sync();
        self.poll_print();
        self.screenshot_tick(&ctx);
        // Tombol kembali Android / Esc di komputer.
        let size = ctx.content_rect().size();
        if self.insets_for != Some(size) {
            // Di awal start Android bisa melapor 0 (jendela belum siap): jangan disimpan, coba lagi sebentar lagi.
            match crate::system_insets() {
                Some((t, b)) if t > 0.0 || cfg!(not(target_os = "android")) => {
                    let ppp = ctx.pixels_per_point();
                    self.insets = (t / ppp, b / ppp);
                    self.insets_for = Some(size);
                }
                _ => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::BrowserBack)) && self.confirm.is_none() {
            let now = ctx.input(|i| i.time);
            if self.scan.is_some() {
                self.scan = None;
            } else if self.nav.len() > 1 {
                self.back();
            } else if now - self.exit_at < 2.0 {
                self.exit_at = -10.0;
                crate::leave_app(&ctx);
            } else {
                self.exit_at = now;
                self.ok("Tekan kembali sekali lagi untuk keluar");
            }
        }
        self.top_bar(ui);
        self.bottom_bar(ui);
        let screen = self.current();
        // Beranda menggambar pita periwinkle selebar layar, jadi panelnya tanpa margin.
        let pad = if screen == Screen::Home { Margin::ZERO } else { Margin { left: 16, right: 16, top: 8, bottom: 0 } };
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(pad))
            .show(ui, |ui| match screen {
                Screen::Home => more::home(self, ui),
                Screen::Pos => pos::pos_screen(self, ui),
                Screen::Products => products::list_screen(self, ui),
                Screen::ProductEdit => products::edit_screen(self, ui),
                Screen::ProductDetail(id) => products::detail_screen(self, ui, &id),
                Screen::StockForm(k) => stock::form_screen(self, ui, k),
                Screen::History => stock::history_screen(self, ui),
                Screen::TxDetail(id) => stock::detail_screen(self, ui, &id),
                Screen::Return(id) => stock::return_screen(self, ui, &id),
                Screen::More => more::more_menu(self, ui),
                Screen::Contacts(k) => more::contacts_screen(self, ui, k),
                Screen::ContactEdit => more::contact_edit(self, ui),
                Screen::Receivables => more::receivables_screen(self, ui),
                Screen::Expenses => more::expenses_screen(self, ui),
                Screen::ExpenseEdit => more::expense_edit(self, ui),
                Screen::Staff => more::staff_screen(self, ui),
                Screen::StaffEdit => more::staff_edit(self, ui),
                Screen::Reports => more::reports_screen(self, ui),
                Screen::Settings => more::settings_screen(self, ui),
                Screen::Categories(k) => more::categories_screen(self, ui, k),
                Screen::PaymentMethods => more::payment_methods_screen(self, ui),
                Screen::Fees => more::fees_screen(self, ui),
                Screen::Sync => more::sync_screen(self, ui),
            });
        self.scan_ui(&ctx);
        self.confirm_ui(&ctx);
        self.toast_ui(&ctx);
    }
}
