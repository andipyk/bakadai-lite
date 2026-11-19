//! Stok Masuk / Keluar / Audit, Riwayat (dengan filter), Detail transaksi (pembayaran, struk, retur).

use super::widgets::*;
use super::{Confirm, BakadaiApp, Screen};
use crate::db::{self, CartLine, NewTx, Product, ProductRow, ProductSort, StockFilter, TxDetail, TxKind, TxRow};
use crate::escpos;
use egui::{Align, Layout, RichText, Ui};

#[derive(Default)]
pub struct StockState {
    pub kind: Option<TxKind>,
    pub lines: Vec<(CartLine, String, String)>, // baris, qty teks, harga teks
    pub search: String,
    pub contact_id: Option<String>,
    pub date: String,
    pub paid_full: bool,
    pub paid: String,
    pub method_id: Option<String>,
    pub note: String,
    // riwayat
    pub h_kind: Option<TxKind>,
    pub h_range: usize,
    pub h_search: String,
    pub h_unpaid: bool,
    pub h_limit: i64,
    h_rev: u64,
    h_key: String,
    pub h_rows: Vec<TxRow>,
    // detail
    pub pay_amount: String,
    pub pay_method: Option<String>,
    pub show_receipt: bool,
    // retur
    pub ret_lines: Vec<(String, String, f64, String, String, f64)>, // product_id, nama, maks, qty, rusak, harga
    pub ret_refund: String,
    pub ret_for: String,
}

impl StockState {
    pub fn open(&mut self, kind: TxKind, product: Option<&Product>) {
        self.kind = Some(kind);
        self.lines.clear();
        self.search.clear();
        self.contact_id = None;
        self.date = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        self.paid_full = true;
        self.paid.clear();
        self.method_id = None;
        self.note.clear();
        if let Some(p) = product {
            self.add(p.id.clone(), p.name.clone(), p.stock, if kind == TxKind::In { p.cost_price } else { p.sell_price });
        }
    }
    fn add(&mut self, id: String, name: String, stock: f64, price: db::Money) {
        if self.lines.iter().any(|l| l.0.product_id == id) {
            return;
        }
        let kind = self.kind.unwrap_or(TxKind::Out);
        let qty = if kind == TxKind::Audit { db::fmt_qty(stock) } else { "1".into() };
        self.lines.push((CartLine { product_id: id, name, qty: 1.0, price, ..Default::default() }, qty, money_str(price)));
    }
}

/// Rentang tanggal riwayat: Hari ini, 7 hari, Bulan ini, 30 hari, 180 hari, Semua.
pub const RANGES: [&str; 6] = ["Hari ini", "7 hari", "Bulan ini", "30 hari terakhir", "180 Hari", "Semua"];

pub fn range_bounds(idx: usize) -> (i64, i64) {
    let now = db::now_ms();
    let (today, end) = db::day_bounds(now);
    match idx {
        0 => (today, end),
        1 => (today - 6 * 86_400_000, end),
        2 => {
            use chrono::{Datelike, TimeZone};
            let d = chrono::Local::now();
            let first = chrono::Local
                .with_ymd_and_hms(d.year(), d.month(), 1, 0, 0, 0)
                .earliest()
                .map(|x| x.timestamp_millis())
                .unwrap_or(today);
            (first, end)
        }
        3 => (today - 29 * 86_400_000, end),
        4 => (today - 179 * 86_400_000, end),
        _ => (0, end + 365 * 86_400_000),
    }
}

fn parse_dt(s: &str) -> Option<i64> {
    use chrono::TimeZone;
    let s = s.trim();
    let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
        .ok()
        .or_else(|| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(12, 0, 0)))?;
    chrono::Local.from_local_datetime(&dt).earliest().map(|x| x.timestamp_millis())
}

/// Pencarian barang untuk ditambahkan ke formulir (maks 8 hasil, hasil diambil dari database).
pub fn product_picker(app: &mut BakadaiApp, ui: &mut Ui, search: &mut String) -> Option<ProductRow> {
    let mut picked = None;
    let resp = ui.add(
        egui::TextEdit::singleline(search)
            .hint_text("Cari / scan kode barang lalu Enter…")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(10, 9)),
    );
    // Mode Barcode Scanner: scanner USB/Bluetooth mengetik kode + Enter.
    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !search.trim().is_empty() {
        if let Some(p) = app.db.product_by_code(search.trim()) {
            picked = app.db.list_products(&p.code, None, StockFilter::All, ProductSort::NameAsc).into_iter().find(|r| r.id == p.id);
            search.clear();
        } else {
            app.err("Kode Barang tidak tersedia");
        }
        resp.request_focus();
    }
    if !search.trim().is_empty() && picked.is_none() {
        let rows = app.db.list_products(search, None, StockFilter::All, ProductSort::NameAsc);
        for r in rows.into_iter().take(8) {
            let sub = format!("{} · stok {}", r.code, db::fmt_qty(r.stock));
            let price = app.money(r.sell_price);
            if list_row(ui, &r.name, &sub, &price, MUTED).clicked() {
                picked = Some(r);
            }
        }
    }
    if picked.is_some() {
        search.clear();
    }
    picked
}

pub fn form_screen(app: &mut BakadaiApp, ui: &mut Ui, kind: TxKind) {
    if app.stock.kind != Some(kind) {
        app.stock.open(kind, None);
    }
    let mut submit = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if kind == TxKind::Audit {
            card(ui, |ui| muted(ui, "Stok Audit adalah penyesuaian jumlah stok tanpa masuk laporan keuangan. Isi jumlah stok hasil hitung."));
        }
        card(ui, |ui| {
            ui.label(RichText::new("Pilih Barang").family(medium()));
            let mut s = std::mem::take(&mut app.stock.search);
            if let Some(r) = product_picker(app, ui, &mut s) {
                let price = if kind == TxKind::In { r.cost_price } else { r.sell_price };
                app.stock.add(r.id, r.name, r.stock, price);
            }
            app.stock.search = s;
        });
        if app.stock.lines.is_empty() {
            empty_state(ui, "Belum ada data yang dipilih", "Cari barang di atas untuk menambahkan.");
        }
        let mut remove = None;
        let money = |m: db::Money| db::fmt_money(m, &app.currency);
        for (i, (line, qty, price)) in app.stock.lines.iter_mut().enumerate() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&line.name).family(medium()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, RED, RED_SOFT).clicked() {
                            remove = Some(i);
                        }
                    });
                });
                ui.columns(2, |c| {
                    let ql = match kind {
                        TxKind::In => "Jumlah Stok Masuk",
                        TxKind::Out => "Jumlah Stok Keluar",
                        _ => "Jumlah Stok Akhir",
                    };
                    field(&mut c[0], ql, qty, "0");
                    if kind != TxKind::Audit {
                        field(&mut c[1], if kind == TxKind::In { "Harga Modal" } else { "Harga Jual" }, price, "0");
                    }
                });
                line.qty = parse_qty(qty);
                line.price = db::money_from_f64(parse_money(price));
                if kind != TxKind::Audit {
                    muted(ui, format!("Subtotal {}", money((line.price as f64 * line.qty).round() as i64)));
                }
            });
        }
        if let Some(i) = remove {
            app.stock.lines.remove(i);
        }
        if kind != TxKind::Audit {
            let lines: Vec<CartLine> = app.stock.lines.iter().map(|l| l.0.clone()).collect();
            let total = db::compute_totals(&lines, false, 0.0, &[]).total;
            card(ui, |ui| {
                let (ck, label) = if kind == TxKind::In { ("vendor", "Vendor (Tidak Wajib)") } else { ("buyer", "Pembeli (Tidak Wajib)") };
                ui.label(RichText::new(label).size(13.0).color(MUTED));
                let contacts = app.db.contacts(ck, "");
                let sel = app
                    .stock
                    .contact_id
                    .as_ref()
                    .and_then(|id| contacts.iter().find(|c| &c.id == id))
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "—".into());
                egui::ComboBox::from_id_salt("contact").width(ui.available_width() - 30.0).selected_text(sel).show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.stock.contact_id, None, "—");
                    for c in &contacts {
                        ui.selectable_value(&mut app.stock.contact_id, Some(c.id.clone()), &c.name);
                    }
                });
                field(ui, "Tanggal (TTTT-BB-HH JJ:MM)", &mut app.stock.date, "");
                kv(ui, "Total Harga", &money(total));
                toggle(ui, &mut app.stock.paid_full, "Bayar Penuh");
                if !app.stock.paid_full {
                    field(ui, "Jumlah Dibayar (sisanya jadi piutang/hutang)", &mut app.stock.paid, "0");
                }
                let methods = app.db.payment_methods();
                if app.stock.method_id.is_none() {
                    app.stock.method_id = methods.iter().find(|m| m.is_default).map(|m| m.id.clone());
                }
                ui.label(RichText::new("Metode Pembayaran").size(13.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    for m in &methods {
                        let on = app.stock.method_id.as_deref() == Some(m.id.as_str());
                        if chip(ui, on, &m.name).clicked() {
                            app.stock.method_id = Some(m.id.clone());
                        }
                    }
                });
            });
        } else {
            card(ui, |ui| field(ui, "Tanggal (TTTT-BB-HH JJ:MM)", &mut app.stock.date, ""));
        }
        card(ui, |ui| field_multiline(ui, "Catatan", &mut app.stock.note, "Tambahkan Catatan"));
        if primary_button_enabled(ui, !app.stock.lines.is_empty(), "Simpan").clicked() {
            submit = true;
        }
        ui.add_space(20.0);
    });
    if submit {
        let Some(ts) = parse_dt(&app.stock.date) else {
            app.err("Format tanggal salah. Contoh: 2026-11-17 09:30");
            return;
        };
        let lines: Vec<CartLine> = app.stock.lines.iter().map(|l| l.0.clone()).collect();
        let total = db::compute_totals(&lines, false, 0.0, &[]).total;
        let paid = if app.stock.paid_full { total } else { db::money_from_f64(parse_money(&app.stock.paid)) };
        let t = NewTx {
            kind: Some(kind),
            ts,
            contact_id: app.stock.contact_id.clone(),
            staff_id: Some(app.staff_id.clone()),
            lines,
            paid,
            payment_method_id: app.stock.method_id.clone(),
            note: app.stock.note.clone(),
            ..Default::default()
        };
        match app.db.create_tx(&t) {
            Ok(_) => {
                app.ok(match kind {
                    TxKind::In => "Stok masuk tersimpan",
                    TxKind::Out => "Stok keluar tersimpan",
                    _ => "Stok audit tersimpan",
                });
                app.changed();
                app.stock.kind = None;
                app.back();
            }
            Err(e) => app.err(e),
        }
    }
}

pub fn history_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    if app.stock.h_limit == 0 {
        app.stock.h_limit = 200;
    }
    ui.add(
        egui::TextEdit::singleline(&mut app.stock.h_search)
            .hint_text("Cari nomor, pembeli, atau barang…")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(10, 9)),
    );
    egui::ScrollArea::horizontal().id_salt("hrange").show(ui, |ui| {
        ui.horizontal(|ui| {
            for (i, r) in RANGES.iter().enumerate() {
                if chip(ui, app.stock.h_range == i, *r).clicked() {
                    app.stock.h_range = i;
                }
            }
        });
    });
    egui::ScrollArea::horizontal().id_salt("hkind").show(ui, |ui| {
        ui.horizontal(|ui| {
            if chip(ui, app.stock.h_kind.is_none(), "Semua jenis").clicked() {
                app.stock.h_kind = None;
            }
            for k in [TxKind::Out, TxKind::In, TxKind::Audit, TxKind::SaleReturn, TxKind::PurchaseReturn] {
                if chip(ui, app.stock.h_kind == Some(k), k.label()).clicked() {
                    app.stock.h_kind = Some(k);
                }
            }
        });
    });
    toggle(ui, &mut app.stock.h_unpaid, "Hanya yang belum lunas");
    let (from, to) = range_bounds(app.stock.h_range);
    let key = format!("{:?}|{}|{}|{}|{}", app.stock.h_kind, app.stock.h_range, app.stock.h_search, app.stock.h_unpaid, app.stock.h_limit);
    if app.stock.h_rev != app.rev || app.stock.h_key != key {
        app.stock.h_rows = app.db.list_tx(app.stock.h_kind, from, to, &app.stock.h_search, app.stock.h_unpaid, app.stock.h_limit);
        app.stock.h_rev = app.rev;
        app.stock.h_key = key;
    }
    let rows = app.stock.h_rows.clone();
    if rows.is_empty() {
        empty_state(ui, "Belum Ada Riwayat", "Transaksi stok dan penjualan akan muncul di sini.");
        return;
    }
    let total_out: db::Money = rows.iter().filter(|r| r.kind == TxKind::Out).map(|r| r.total).sum();
    muted(ui, format!("{} transaksi · penjualan {}", rows.len(), app.money(total_out)));
    let n = rows.len();
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show_rows(ui, 66.0, n + 1, |ui, range| {
        for i in range {
            if i == n {
                if n as i64 >= app.stock.h_limit && ui.button("Muat lebih banyak").clicked() {
                    app.stock.h_limit += 200;
                }
                continue;
            }
            let r = &rows[i];
            let who = if r.contact.is_empty() { String::new() } else { format!(" · {}", r.contact) };
            let sub = format!("{} · {}{}", db::fmt_dt(r.ts), r.number, who);
            let title = if r.summary.is_empty() { r.kind.label().to_string() } else { format!("{}: {}", r.kind.label(), r.summary) };
            let right = match r.kind {
                TxKind::Audit => format!("±{}", db::fmt_qty(r.qty)),
                _ => app.money(r.total),
            };
            let resp = list_row_icon(ui, Some(kind_icon(r.kind)), &title, &sub, &right, TEXT);
            if matches!(r.kind, TxKind::Out | TxKind::In) && r.pay_status() != "Lunas" {
                let p = egui::pos2(resp.rect.right() - 12.0, resp.rect.bottom() - 12.0);
                ui.painter().text(p, egui::Align2::RIGHT_CENTER, if r.paid > 0 { "Sebagian" } else { "Piutang" }, egui::FontId::proportional(11.0), RED);
            }
            if resp.clicked() {
                app.stock.show_receipt = false;
                app.stock.pay_amount.clear();
                app.go(Screen::TxDetail(r.id.clone()));
            }
        }
    }));
}

/// Struk teks lebar tetap (dipakai untuk pratinjau, salin/bagikan, dan printer ESC/POS).
pub fn receipt_text(app: &BakadaiApp, d: &TxDetail) -> String {
    let paper: u32 = app.db.setting("paper_mm", "58").parse().unwrap_or(58);
    let w = escpos::columns_for_paper(paper);
    let m = |v: db::Money| db::fmt_money(v, &app.currency);
    let mut s = String::new();
    s.push_str(&escpos::center(&app.business_name, w));
    s.push('\n');
    let addr = app.db.setting("business_address", "");
    if !addr.is_empty() {
        s.push_str(&escpos::center(&addr, w));
        s.push('\n');
    }
    s.push_str(&escpos::rule(w));
    s.push('\n');
    s.push_str(&escpos::lr(&d.number, &db::fmt_dt(d.ts), w));
    s.push('\n');
    if !d.contact.is_empty() {
        s.push_str(&format!("{} {}\n", if d.kind == Some(TxKind::In) { "Vendor:" } else { "Pembeli:" }, d.contact));
    }
    if !d.staff.is_empty() {
        s.push_str(&format!("Kasir: {}\n", d.staff));
    }
    s.push_str(&escpos::rule(w));
    s.push('\n');
    for l in &d.lines {
        s.push_str(&format!("{}\n", l.name.chars().take(w).collect::<String>()));
        let tier = l.tier.as_ref().map(|t| format!(" ({})", t)).unwrap_or_default();
        s.push_str(&escpos::lr(
            &format!("  {} x {}{}", db::fmt_qty(l.qty), m(l.price), tier),
            &m((l.price as f64 * l.qty).round() as i64),
            w,
        ));
        s.push('\n');
    }
    s.push_str(&escpos::rule(w));
    s.push('\n');
    s.push_str(&escpos::lr("Subtotal", &m(d.subtotal), w));
    s.push('\n');
    if d.discount > 0 {
        s.push_str(&escpos::lr("Diskon", &format!("-{}", m(d.discount)), w));
        s.push('\n');
    }
    for (n, a) in &d.fee_lines {
        s.push_str(&escpos::lr(n, &m(*a), w));
        s.push('\n');
    }
    s.push_str(&escpos::lr("TOTAL", &m(d.total), w));
    s.push('\n');
    s.push_str(&escpos::lr(&format!("Dibayar ({})", if d.method.is_empty() { "-" } else { &d.method }), &m(d.paid), w));
    s.push('\n');
    if d.paid < d.total {
        s.push_str(&escpos::lr("Belum Dibayar", &m(d.total - d.paid), w));
        s.push('\n');
    }
    let footer = app.db.setting("receipt_footer", "Terima kasih!");
    if !footer.is_empty() {
        s.push_str(&escpos::rule(w));
        s.push('\n');
        s.push_str(&escpos::center(&footer, w));
        s.push('\n');
    }
    s
}

pub fn detail_screen(app: &mut BakadaiApp, ui: &mut Ui, id: &str) {
    let Some(d) = app.db.tx_detail(id) else {
        empty_state(ui, "Transaksi tidak ditemukan", "");
        return;
    };
    let kind = d.kind.unwrap_or(TxKind::Out);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(kind.label()).family(medium()).size(17.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if matches!(kind, TxKind::Out | TxKind::In) {
                        pay_pill(ui, db::pay_status(d.total, d.paid));
                    }
                });
            });
            muted(ui, format!("{} · {}", d.number, db::fmt_dt(d.ts)));
            if !d.contact.is_empty() {
                kv(ui, if kind == TxKind::In { "Vendor" } else { "Pembeli" }, &d.contact);
            }
            if !d.staff.is_empty() {
                kv(ui, "Dibuat oleh", &d.staff);
            }
            if !d.note.is_empty() {
                kv(ui, "Catatan", &d.note);
            }
        });
        card(ui, |ui| {
            for l in &d.lines {
                let right = if kind == TxKind::Audit {
                    format!("{}{}", if l.qty >= 0.0 { "+" } else { "" }, db::fmt_qty(l.qty))
                } else {
                    app.money((l.price as f64 * l.qty).round() as i64)
                };
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&l.name).family(medium()));
                        if kind != TxKind::Audit {
                            let mut sub = format!("{} × {}", db::fmt_qty(l.qty), app.money(l.price));
                            if let Some(t) = &l.tier {
                                sub.push_str(&format!(" · {}", t));
                            }
                            if l.damaged > 0.0 {
                                sub.push_str(&format!(" · rusak {}", db::fmt_qty(l.damaged)));
                            }
                            muted(ui, sub);
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(right).family(medium()));
                    });
                });
            }
            if kind != TxKind::Audit {
                ui.separator();
                kv(ui, "Subtotal", &app.money(d.subtotal));
                if d.discount > 0 {
                    kv(ui, "Diskon", &format!("-{}", app.money(d.discount)));
                }
                for (n, a) in &d.fee_lines {
                    kv(ui, n, &app.money(*a));
                }
                kv(ui, "Total", &app.money(d.total));
                kv(ui, "Jumlah Dibayar", &app.money(d.paid));
                if d.total > d.paid {
                    kv_color(ui, "Belum Dibayar", &app.money(d.total - d.paid), RED);
                }
                if kind == TxKind::Out && app.can("view_cost") {
                    kv_color(ui, "Untung", &app.money(d.subtotal - d.discount - d.cost_total), GREEN);
                }
                if d.refunded > 0 {
                    kv_color(ui, "Total Retur", &app.money(d.refunded), ORANGE);
                }
            }
        });
        if matches!(kind, TxKind::Out | TxKind::In) && d.total > d.paid {
            card(ui, |ui| {
                ui.label(RichText::new("Tambah Pembayaran").family(medium()));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.stock.pay_amount).hint_text(money_str(d.total - d.paid)).desired_width(140.0));
                    let methods = app.db.payment_methods();
                    if app.stock.pay_method.is_none() {
                        app.stock.pay_method = methods.first().map(|m| m.id.clone());
                    }
                    let sel = methods.iter().find(|m| Some(&m.id) == app.stock.pay_method.as_ref()).map(|m| m.name.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("paym").selected_text(sel).show_ui(ui, |ui| {
                        for m in &methods {
                            ui.selectable_value(&mut app.stock.pay_method, Some(m.id.clone()), &m.name);
                        }
                    });
                });
                if primary_button(ui, "Simpan Pembayaran").clicked() {
                    let amt = if app.stock.pay_amount.trim().is_empty() {
                        d.total - d.paid
                    } else {
                        db::money_from_f64(parse_money(&app.stock.pay_amount))
                    };
                    match app.db.add_payment(&d.id, amt, app.stock.pay_method.clone(), db::now_ms()) {
                        Ok(_) => {
                            app.stock.pay_amount.clear();
                            app.ok("Pembayaran berhasil ditambahkan");
                            app.changed();
                        }
                        Err(e) => app.err(e),
                    }
                }
            });
        }
        if !d.payments.is_empty() {
            section_title(ui, "Riwayat Pembayaran");
            list_group(ui, |ui| {
                for (_, ts, amt, m) in &d.payments {
                    list_row(ui, &app.money(*amt), &format!("{} · {}", db::fmt_dt(*ts), m), "", MUTED);
                }
            });
        }
        if kind != TxKind::Audit {
            if secondary_button(ui, if app.stock.show_receipt { "Sembunyikan Struk" } else { "Lihat Struk" }).clicked() {
                app.stock.show_receipt = !app.stock.show_receipt;
            }
            if app.stock.show_receipt {
                let text = receipt_text(app, &d);
                card(ui, |ui| {
                    ui.label(RichText::new(&text).monospace().size(11.5));
                });
                ui.columns(2, |c| {
                    if secondary_button(&mut c[0], "Salin").clicked() {
                        c[0].ctx().copy_text(text.clone());
                        app.ok("Disalin ke papan klip Anda!");
                    }
                    if secondary_button(&mut c[1], "Cetak").clicked() {
                        app.print_receipt(c[1].ctx(), &text);
                    }
                });
            }
        }
        if kind == TxKind::Out && secondary_button(ui, "Buat Retur").clicked() {
            open_return(app, &d);
            app.go(Screen::Return(d.id.clone()));
        }
        if app.can("history_edit") && danger_button(ui, "Hapus Riwayat").clicked() {
            app.confirm = Some(("Stok akan dikembalikan seperti sebelum transaksi ini.".into(), Confirm::DeleteTx(d.id.clone())));
        }
        ui.add_space(20.0);
    });
}

fn open_return(app: &mut BakadaiApp, d: &TxDetail) {
    app.stock.ret_for = d.id.clone();
    app.stock.ret_lines = d
        .lines
        .iter()
        .map(|l| {
            let done: f64 = d.returned.iter().filter(|r| r.0 == l.product_id).map(|r| r.1).sum();
            (l.product_id.clone(), l.name.clone(), (l.qty - done).max(0.0), String::new(), String::new(), l.price as f64)
        })
        .collect();
    app.stock.ret_refund.clear();
}

pub fn return_screen(app: &mut BakadaiApp, ui: &mut Ui, id: &str) {
    let Some(d) = app.db.tx_detail(id) else { return };
    if app.stock.ret_for != id {
        open_return(app, &d);
    }
    let mut submit = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            muted(ui, "Kondisi baik: barang kembali ke stok dan bisa dijual kembali. Rusak/cacat: barang tidak kembali ke stok dan harga modalnya dihitung sebagai kerugian.");
        });
        let mut suggested: f64 = 0.0;
        for l in app.stock.ret_lines.iter_mut() {
            card(ui, |ui| {
                ui.label(RichText::new(&l.1).family(medium()));
                muted(ui, format!("Sisa retur {}", db::fmt_qty(l.2)));
                ui.columns(2, |c| {
                    field(&mut c[0], "Jumlah Retur", &mut l.3, "0");
                    field(&mut c[1], "Jumlah Rusak", &mut l.4, "0");
                });
                suggested += parse_qty(&l.3) * l.5;
            });
        }
        let max_refund = d.paid.max(d.total) - d.refunded;
        card(ui, |ui| {
            field(ui, "Pengembalian Dana", &mut app.stock.ret_refund, &money_str(suggested.round() as i64));
            muted(ui, format!("Maks. refund {}", app.money(max_refund)));
        });
        if primary_button(ui, "Buat Retur").clicked() {
            submit = true;
        }
    });
    if submit {
        let mut lines = vec![];
        let mut suggested = 0.0;
        for l in &app.stock.ret_lines {
            let q = parse_qty(&l.3);
            let dmg = parse_qty(&l.4);
            if q <= 0.0 {
                continue;
            }
            if q > l.2 + 1e-9 {
                app.err("Jumlah retur tidak boleh melebihi transaksi awal");
                return;
            }
            suggested += q * l.5;
            lines.push(CartLine { product_id: l.0.clone(), name: l.1.clone(), qty: q, damaged: dmg, price: l.5 as i64, ..Default::default() });
        }
        if lines.is_empty() {
            app.err("Pilih minimal satu barang");
            return;
        }
        let refund = if app.stock.ret_refund.trim().is_empty() {
            suggested.round() as i64
        } else {
            db::money_from_f64(parse_money(&app.stock.ret_refund))
        };
        if refund > d.paid.max(d.total) - d.refunded {
            app.err("Nominal refund tidak boleh melebihi batas refund transaksi awal");
            return;
        }
        let t = NewTx {
            kind: Some(TxKind::SaleReturn),
            ts: db::now_ms(),
            contact_id: d.contact_id.clone(),
            staff_id: Some(app.staff_id.clone()),
            lines,
            ref_id: Some(d.id.clone()),
            refund,
            note: format!("Retur dari {}", d.number),
            ..Default::default()
        };
        match app.db.create_tx(&t) {
            Ok(_) => {
                app.ok("Retur dibuat");
                app.changed();
                app.stock.ret_for.clear();
                app.back();
            }
            Err(e) => app.err(e),
        }
    }
}
