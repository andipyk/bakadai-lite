//! Mode Kasir: pilih barang (grid), keranjang, diskon, biaya tambahan, pembayaran, kembalian, struk.
//! Di HP tampil satu kolom; di tablet (lebar > 720 px) barang kiri, keranjang kanan.

use super::products::product_thumb;
use super::widgets::*;
use super::{Confirm, BakadaiApp, Screen};
use crate::db::{self, CartLine, FeeLine, Money, NewTx, PriceTier, Product, ProductRow, ProductSort, StockFilter, TxKind};
use egui::{Align, Color32, Layout, RichText, Sense, Ui, Vec2};

#[derive(Clone, Default)]
pub struct PosLine {
    pub product_id: String,
    pub name: String,
    pub qty: f64,
    pub sell_price: Money,
    pub tiers: Vec<PriceTier>,
    pub stock: f64,
}

impl PosLine {
    pub fn price(&self) -> (Money, Option<String>) {
        db::tier_price(self.sell_price, &self.tiers, self.qty)
    }
}

#[derive(Default)]
pub struct PosState {
    pub cart: Vec<PosLine>,
    pub search: String,
    pub category: Option<String>,
    pub discount_pct: bool,
    pub discount: String,
    pub fee_on: Vec<String>,
    pub buyer: Option<String>,
    pub method: Option<String>,
    pub cash: String,
    pub note: String,
    pub checkout: bool,
    pub last_tx: Option<String>,
    rev: u64,
    key: String,
    rows: Vec<ProductRow>,
}

impl PosState {
    pub fn add_product(&mut self, p: &Product, qty: f64) {
        if let Some(l) = self.cart.iter_mut().find(|l| l.product_id == p.id) {
            l.qty += qty;
            return;
        }
        self.cart.push(PosLine {
            product_id: p.id.clone(),
            name: p.name.clone(),
            qty,
            sell_price: p.sell_price,
            tiers: p.tiers.clone(),
            stock: p.stock,
        });
    }
    /// Kurangi 1; baris hilang bila jumlahnya habis.
    pub fn remove_one(&mut self, product_id: &str) {
        if let Some(i) = self.cart.iter().position(|l| l.product_id == product_id) {
            if self.cart[i].qty <= 1.0 {
                self.cart.remove(i);
            } else {
                self.cart[i].qty -= 1.0;
            }
        }
    }
    pub fn clear(&mut self) {
        self.cart.clear();
        self.discount.clear();
        self.fee_on.clear();
        self.buyer = None;
        self.cash.clear();
        self.note.clear();
        self.checkout = false;
    }
    fn lines(&self) -> Vec<CartLine> {
        self.cart
            .iter()
            .map(|l| {
                let (price, tier) = l.price();
                CartLine { product_id: l.product_id.clone(), name: l.name.clone(), qty: l.qty, price, tier, damaged: 0.0 }
            })
            .collect()
    }
}

fn fees_for(app: &BakadaiApp) -> Vec<FeeLine> {
    app.db
        .fee_templates()
        .into_iter()
        .filter(|f| app.pos.fee_on.contains(&f.id))
        .map(|f| FeeLine { name: f.name, kind: f.kind, value: f.value, include_profit: f.include_profit })
        .collect()
}

pub fn pos_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    if let Some(id) = app.pos.last_tx.clone() {
        success(app, ui, &id);
        return;
    }
    let wide = ui.available_width() > 720.0;
    if wide {
        ui.columns(2, |c| {
            catalog(app, &mut c[0]);
            checkout(app, &mut c[1]);
        });
    } else if app.pos.checkout {
        checkout(app, ui);
    } else {
        let bar_h = if app.pos.cart.is_empty() { 0.0 } else { 76.0 };
        let avail = ui.available_height() - bar_h;
        ui.allocate_ui(Vec2::new(ui.available_width(), avail), |ui| catalog(app, ui));
        if !app.pos.cart.is_empty() {
            let t = db::compute_totals(&app.pos.lines(), false, 0.0, &[]);
            let n: f64 = app.pos.cart.iter().map(|l| l.qty).sum();
            let w = ui.available_width();
            // Pil tinta: jumlah + total di kiri, tombol "Bayar →" periwinkle di kanan.
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 64.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 32, PRIMARY);
            p.text(rect.left_center() + Vec2::new(22.0, -10.0), egui::Align2::LEFT_CENTER, format!("{} barang", db::fmt_qty(n)), egui::FontId::proportional(13.0), ACCENT);
            p.text(rect.left_center() + Vec2::new(22.0, 10.0), egui::Align2::LEFT_CENTER, app.money(t.subtotal), egui::FontId::new(18.0, medium()), Color32::WHITE);
            let pay = egui::Rect::from_min_max(egui::pos2(rect.right() - 6.0 - 116.0, rect.top() + 6.0), egui::pos2(rect.right() - 6.0, rect.bottom() - 6.0));
            p.rect_filled(pay, 26, if resp.hovered() { PRIMARY_SOFT } else { ACCENT });
            p.text(egui::pos2(pay.left() + 22.0, pay.center().y), egui::Align2::LEFT_CENTER, "Bayar", egui::FontId::new(16.0, medium()), TEXT);
            paint_icon(p, Icon::Next, egui::pos2(pay.right() - 24.0, pay.center().y), 18.0, TEXT);
            if resp.clicked() {
                app.pos.checkout = true;
            }
        }
    }
}

fn catalog(app: &mut BakadaiApp, ui: &mut Ui) {
    let resp = ui.add(
        egui::TextEdit::singleline(&mut app.pos.search)
            .hint_text("Cari barang / scan barcode lalu Enter")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(10, 9)),
    );
    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let code = app.pos.search.trim().to_string();
        if let Some(p) = app.db.product_by_code(&code) {
            app.pos.add_product(&p, 1.0);
            app.pos.search.clear();
            app.ok(format!("{} ditambahkan", p.name));
        } else if !code.is_empty() {
            app.err("Kode Barang tidak tersedia");
        }
        resp.request_focus();
    }
    let cats = app.db.categories("product");
    egui::ScrollArea::horizontal().id_salt("poscats").show(ui, |ui| {
        ui.horizontal(|ui| {
            if chip(ui, app.pos.category.is_none(), "Semua").clicked() {
                app.pos.category = None;
            }
            for c in &cats {
                if chip(ui, app.pos.category.as_deref() == Some(c.id.as_str()), &c.name).clicked() {
                    app.pos.category = Some(c.id.clone());
                }
            }
        });
    });
    let key = format!("{}|{:?}", app.pos.search, app.pos.category);
    if app.pos.rev != app.rev || app.pos.key != key {
        app.pos.rows = app.db.list_products(&app.pos.search, app.pos.category.as_deref(), StockFilter::All, ProductSort::NameAsc);
        app.pos.rev = app.rev;
        app.pos.key = key;
    }
    if app.pos.rows.is_empty() {
        empty_state(ui, "Tidak ada produk ditemukan", "Silakan tambah produk dan buat transaksi baru.");
        return;
    }
    let w = ui.available_width();
    let cols = ((w / 118.0).floor() as usize).clamp(2, 6);
    let gap = 8.0;
    let tile_w = (w - gap * (cols as f32 - 1.0)) / cols as f32;
    let tile_h = tile_w * 0.62 + 52.0;
    let rows = app.pos.rows.len().div_ceil(cols);
    egui::ScrollArea::vertical().id_salt("posgrid").auto_shrink([false, false]).show_rows(ui, tile_h, rows, |ui, range| {
        for r in range {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for c in 0..cols {
                    let i = r * cols + c;
                    let Some(p) = app.pos.rows.get(i).cloned() else { break };
                    tile(app, ui, &p, tile_w, tile_h);
                }
            });
        }
    });
}

fn tile(app: &mut BakadaiApp, ui: &mut Ui, p: &ProductRow, w: f32, h: f32) {
    let in_cart = app.pos.cart.iter().find(|l| l.product_id == p.id).map(|l| l.qty).unwrap_or(0.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 24, CARD);
    if in_cart > 0.0 {
        painter.rect_stroke(rect, 24, egui::Stroke::new(2.0, PRIMARY), egui::StrokeKind::Inside);
    }
    let img_h = w * 0.62;
    let img_rect = egui::Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(w - 20.0, img_h - 10.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(img_rect).layout(Layout::top_down(Align::Center)));
    product_thumb(app, &mut child, &p.name, p.thumb.as_ref(), (img_h - 8.0).min(w - 12.0));
    let nf = egui::FontId::proportional(14.0);
    let name = fit_text(&painter, &p.name, &nf, w - 20.0);
    painter.text(rect.left_top() + Vec2::new(10.0, img_h + 8.0), egui::Align2::LEFT_TOP, name, nf, TEXT);
    painter.text(rect.left_bottom() + Vec2::new(10.0, -14.0), egui::Align2::LEFT_CENTER, app.money(p.sell_price), egui::FontId::new(15.0, medium()), TEXT);
    let stock_col = if p.stock <= 0.0 { RED } else { MUTED };
    painter.text(rect.right_bottom() + Vec2::new(-10.0, -14.0), egui::Align2::RIGHT_CENTER, db::fmt_qty(p.stock), egui::FontId::proportional(12.5), stock_col);
    if in_cart > 0.0 {
        let c = rect.right_top() + Vec2::new(-20.0, 20.0);
        painter.circle_filled(c, 14.0, PRIMARY);
        painter.text(c, egui::Align2::CENTER_CENTER, db::fmt_qty(in_cart), egui::FontId::proportional(12.0), Color32::WHITE);
    }
    // Klik didaftarkan SETELAH foto digambar, supaya seluruh kotak (termasuk foto) bisa diketuk.
    let resp = ui.interact(rect, ui.scope_id().with(("tile", &p.id)), Sense::click());
    let minus = if in_cart > 0.0 {
        let c = rect.left_top() + Vec2::new(20.0, 20.0);
        painter.circle_filled(c, 14.0, RED);
        painter.line_segment([c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)], egui::Stroke::new(2.5, Color32::WHITE));
        let r = egui::Rect::from_center_size(c, Vec2::splat(44.0));
        Some(ui.interact(r, ui.scope_id().with(("minus", &p.id)), Sense::click()).on_hover_text("Kurangi 1"))
    } else {
        None
    };
    if minus.is_some_and(|m| m.clicked()) {
        app.pos.remove_one(&p.id);
    } else if resp.clicked() {
        if let Some(full) = app.db.product(&p.id) {
            if full.stock - in_cart < 1.0 && !app.db.setting_bool("allow_negative", false) {
                app.err(format!("Stok tidak mencukupi: {}", full.name));
            } else {
                app.pos.add_product(&full, 1.0);
            }
        }
    }
}

fn checkout(app: &mut BakadaiApp, ui: &mut Ui) {
    let wide = ui.available_width() > 360.0 && ui.ctx().content_rect().width() > 720.0;
    let mut pay = false;
    egui::ScrollArea::vertical().id_salt("checkout").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            if !wide && small_button(ui, "< Barang").clicked() {
                app.pos.checkout = false;
            }
            ui.label(RichText::new("Keranjang").family(medium()).size(16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !app.pos.cart.is_empty() && ui.link("Kosongkan").clicked() {
                    app.confirm = Some(("Kosongkan keranjang?".into(), Confirm::ClearCart));
                }
            });
        });
        if app.pos.cart.is_empty() {
            empty_state(ui, "Keranjang kosong", "Pilih barang di samping kiri");
            return;
        }
        let mut remove = None;
        let cur = app.currency.clone();
        for (i, l) in app.pos.cart.iter_mut().enumerate() {
            card(ui, |ui| {
                let (price, tier) = l.price();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&l.name).family(medium()));
                        let mut s = db::fmt_money(price, &cur);
                        if let Some(t) = &tier {
                            s.push_str(&format!(" · {}", t));
                        }
                        ui.label(RichText::new(s).size(12.5).color(if tier.is_some() { GREEN } else { MUTED }));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_button(ui, "+").clicked() {
                            l.qty += 1.0;
                        }
                        ui.label(RichText::new(db::fmt_qty(l.qty)).family(medium()).size(16.0));
                        if small_button(ui, "−").clicked() {
                            if l.qty <= 1.0 {
                                remove = Some(i);
                            } else {
                                l.qty -= 1.0;
                            }
                        }
                    });
                });
                if l.qty > l.stock {
                    ui.label(RichText::new(format!("Stok tersedia {}", db::fmt_qty(l.stock))).size(12.0).color(RED));
                }
            });
        }
        if let Some(i) = remove {
            app.pos.cart.remove(i);
        }
        let fees = fees_for(app);
        let lines = app.pos.lines();
        let disc_val = parse_money(&app.pos.discount);
        let t = db::compute_totals(&lines, app.pos.discount_pct, disc_val, &fees);
        card(ui, |ui| {
            ui.label(RichText::new("Diskon (Tidak Wajib)").size(13.0).color(MUTED));
            ui.horizontal(|ui| {
                let mut pct = app.pos.discount_pct;
                segmented(ui, &mut pct, &[(false, "Rp"), (true, "%")]);
                app.pos.discount_pct = pct;
                ui.add(egui::TextEdit::singleline(&mut app.pos.discount).hint_text("0").desired_width(f32::INFINITY));
            });
            if app.pos.discount_pct && disc_val > db::MAX_DISCOUNT_PCT {
                ui.label(RichText::new("Maksimum diskon adalah 100% dari total harga").color(RED).size(12.0));
            }
            let templates = app.db.fee_templates();
            if !templates.is_empty() {
                ui.label(RichText::new("Biaya Tambahan (Tidak Wajib)").size(13.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    for f in templates {
                        let on = app.pos.fee_on.contains(&f.id);
                        let label = if f.kind == "percent" { format!("{} {}%", f.name, f.value) } else { format!("{} {}", f.name, db::fmt_money(db::money_from_f64(f.value), &cur)) };
                        if chip(ui, on, label).clicked() {
                            if on {
                                app.pos.fee_on.retain(|x| x != &f.id);
                            } else {
                                app.pos.fee_on.push(f.id.clone());
                            }
                        }
                    }
                });
            }
            ui.separator();
            kv(ui, "Subtotal", &app.money(t.subtotal));
            if t.discount > 0 {
                kv(ui, "Diskon", &format!("-{}", app.money(t.discount)));
            }
            if t.fees > 0 {
                kv(ui, "Biaya Tambahan", &app.money(t.fees));
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Total").family(medium()).size(17.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(app.money(t.total)).family(medium()).size(19.0).color(PRIMARY));
                });
            });
        });
        card(ui, |ui| {
            let buyers = app.db.contacts("buyer", "");
            let sel = app.pos.buyer.as_ref().and_then(|id| buyers.iter().find(|c| &c.id == id)).map(|c| c.name.clone()).unwrap_or_else(|| "Pembeli Toko".into());
            ui.label(RichText::new("Pembeli (Tidak Wajib)").size(13.0).color(MUTED));
            egui::ComboBox::from_id_salt("buyer").width(ui.available_width() - 30.0).selected_text(sel).show_ui(ui, |ui| {
                ui.selectable_value(&mut app.pos.buyer, None, "Pembeli Toko");
                for c in &buyers {
                    ui.selectable_value(&mut app.pos.buyer, Some(c.id.clone()), &c.name);
                }
            });
            let methods = app.db.payment_methods();
            if app.pos.method.is_none() {
                app.pos.method = methods.iter().find(|m| m.is_default).map(|m| m.id.clone());
            }
            ui.label(RichText::new("Metode Pembayaran").size(13.0).color(MUTED));
            ui.horizontal_wrapped(|ui| {
                for m in &methods {
                    if chip(ui, app.pos.method.as_deref() == Some(m.id.as_str()), &m.name).clicked() {
                        app.pos.method = Some(m.id.clone());
                    }
                }
            });
            ui.label(RichText::new("Uang Diterima").size(13.0).color(MUTED));
            ui.add(egui::TextEdit::singleline(&mut app.pos.cash).hint_text(money_str(t.total)).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10, 9)));
            ui.horizontal_wrapped(|ui| {
                if small_button(ui, "Uang Pas").clicked() {
                    app.pos.cash = money_str(t.total);
                }
                for v in quick_cash(t.total) {
                    if small_button(ui, &db::fmt_money(v, "").trim().to_string()).clicked() {
                        app.pos.cash = money_str(v);
                    }
                }
            });
            let cash = if app.pos.cash.trim().is_empty() { t.total } else { db::money_from_f64(parse_money(&app.pos.cash)) };
            if cash >= t.total {
                kv_color(ui, "Kembalian", &app.money(cash - t.total), GREEN);
            } else {
                kv_color(ui, "Belum Dibayar (Piutang)", &app.money(t.total - cash), RED);
                if app.pos.buyer.is_none() {
                    ui.label(RichText::new("Tips: pilih pembeli agar piutang mudah ditagih.").size(12.0).color(ORANGE));
                }
            }
            field(ui, "Catatan", &mut app.pos.note, "Tambahkan Catatan");
        });
        if primary_button(ui, &format!("Bayar {}", app.money(t.total))).clicked() {
            pay = true;
        }
        ui.add_space(16.0);
    });
    if pay {
        let fees = fees_for(app);
        let lines = app.pos.lines();
        let t = db::compute_totals(&lines, app.pos.discount_pct, parse_money(&app.pos.discount), &fees);
        let cash = if app.pos.cash.trim().is_empty() { t.total } else { db::money_from_f64(parse_money(&app.pos.cash)) };
        let tx = NewTx {
            kind: Some(TxKind::Out),
            ts: db::now_ms(),
            contact_id: app.pos.buyer.clone(),
            staff_id: Some(app.staff_id.clone()),
            lines,
            discount_pct: app.pos.discount_pct,
            discount_value: parse_money(&app.pos.discount),
            fees,
            paid: cash,
            payment_method_id: app.pos.method.clone(),
            note: app.pos.note.clone(),
            ..Default::default()
        };
        match app.db.create_tx(&tx) {
            Ok(id) => {
                app.changed();
                app.pos.clear();
                app.pos.last_tx = Some(id.clone());
                if app.db.setting_bool("auto_print", false) && !app.db.setting("printer_addr", "").is_empty() {
                    if let Some(d) = app.db.tx_detail(&id) {
                        let text = super::stock::receipt_text(app, &d);
                        app.print_receipt(ui.ctx(), &text);
                    }
                }
            }
            Err(e) => app.err(e),
        }
    }
}

/// Tombol uang cepat: pecahan rupiah di atas total (20rb, 50rb, 100rb…).
fn quick_cash(total: Money) -> Vec<Money> {
    let mut out = vec![];
    for step in [1_000_000i64, 2_000_000, 5_000_000, 10_000_000] {
        let v = ((total + step - 1) / step) * step;
        if v > total && !out.contains(&v) {
            out.push(v);
        }
    }
    out.truncate(3);
    out
}

fn success(app: &mut BakadaiApp, ui: &mut Ui, id: &str) {
    let Some(d) = app.db.tx_detail(id) else {
        app.pos.last_tx = None;
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            let (r, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 30.0, GREEN_SOFT);
            paint_icon(ui.painter(), Icon::Check, r.center(), 34.0, GREEN);
            ui.label(RichText::new("Transaksi berhasil diselesaikan!").family(medium()).size(18.0));
            ui.label(RichText::new(app.money(d.total)).size(22.0).family(medium()).color(PRIMARY));
        });
        let text = super::stock::receipt_text(app, &d);
        card(ui, |ui| {
            ui.label(RichText::new(&text).monospace().size(11.5));
        });
        ui.columns(2, |c| {
            if secondary_button(&mut c[0], "Salin Struk").clicked() {
                c[0].ctx().copy_text(text.clone());
                app.ok("Disalin ke papan klip Anda!");
            }
            if secondary_button(&mut c[1], "Detail").clicked() {
                app.pos.last_tx = None;
                app.go(Screen::TxDetail(d.id.clone()));
            }
        });
        if primary_button(ui, "Transaksi Baru").clicked() {
            app.pos.last_tx = None;
        }
    });
}
