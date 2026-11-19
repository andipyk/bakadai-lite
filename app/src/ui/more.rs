//! Beranda, menu Lainnya, Pembeli/Vendor, Piutang, Pengeluaran, Staff (tanpa batas), Laporan,
//! Pengaturan (struk, printer, stok), Kategori, Metode Pembayaran, Biaya Tambahan, Server & Sinkron.

use super::widgets::*;
use super::{Confirm, BakadaiApp, Screen};
use crate::db::{self, Contact, Dashboard, Expense, FeeTemplate, Report, Staff, TxKind};
use egui::{Align, Layout, RichText, Ui};

#[derive(Default)]
pub struct HomeState {
    rev: u64,
    dash: Dashboard,
}

#[derive(Default)]
pub struct MoreState {
    pub contact: Contact,
    pub c_search: String,
    pub expense: Expense,
    pub expense_amount: String,
    pub expense_date: String,
    pub e_range: usize,
    pub staff: Staff,
    pub r_range: usize,
    r_key: String,
    r_rev: u64,
    report: Report,
    pub cat_name: String,
    pub cat_edit: Option<String>,
    pub pm_name: String,
    pub fee: FeeTemplate,
    pub fee_value: String,
    pub api_email: String,
    pub api_password: String,
    pub export_msg: String,
    settings_loaded: bool,
    s_name: String,
    s_address: String,
    s_currency: String,
    s_footer: String,
    s_paper: String,
    s_printer: String,
    bt_list: Vec<(String, String)>,
    s_lock: String,
    s_api: String,
}

pub fn home(app: &mut BakadaiApp, ui: &mut Ui) {
    if app.home.rev != app.rev {
        app.home.dash = app.db.dashboard();
        app.home.rev = app.rev;
    }
    let d = app.home.dash.clone();
    {
        // Pita periwinkle (v2), tetap di atas (tidak ikut digulir) supaya menyatu dengan app bar: angka penjualan besar + aksi bulat tinta, menyambung dari app bar.
        egui::Frame::new()
            .fill(ACCENT)
            .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: 32, se: 32 })
            .inner_margin(egui::Margin { left: 20, right: 20, top: 4, bottom: 20 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new("Penjualan hari ini").size(14.0).color(ACCENT_INK));
                    ui.label(RichText::new(app.money(d.sales_today)).size(40.0).family(medium()));
                    let mut meta = format!("{} transaksi", d.tx_today);
                    if app.can("view_cost") {
                        meta = format!("Untung {} · {}", app.money(d.profit_today), meta);
                    }
                    ui.label(RichText::new(meta).size(14.0).color(ACCENT_INK));
                });
                ui.add_space(14.0);
                let qw = (ui.available_width() - 3.0 * 8.0) / 4.0;
                ui.horizontal(|ui| {
                    if app.can("pos") && quick_action(ui, qw, Icon::Pos, "Kasir", PRIMARY).clicked() {
                        app.tab(Screen::Pos);
                    }
                    if app.can("stock_in") && quick_action(ui, qw, Icon::In, "Stok masuk", GREEN).clicked() {
                        app.stock.open(TxKind::In, None);
                        app.go(Screen::StockForm(TxKind::In));
                    }
                    if app.can("stock_out") && quick_action(ui, qw, Icon::Out, "Stok keluar", RED).clicked() {
                        app.stock.open(TxKind::Out, None);
                        app.go(Screen::StockForm(TxKind::Out));
                    }
                    if app.can("audit") && quick_action(ui, qw, Icon::Check, "Audit", PURPLE).clicked() {
                        app.stock.open(TxKind::Audit, None);
                        app.go(Screen::StockForm(TxKind::Audit));
                    }
                });
            });
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 12 }).show(ui, |ui| {
        let w = ui.available_width();
        let half = (w - 10.0) / 2.0;
        section_title(ui, "Pengingat Stok");
        // Satu panel berisi baris bertitik warna (v2); baris stok rendah/habis langsung membuka daftar barangnya.
        list_group(ui, |ui| {
            let rows = [
                (ORANGE, ORANGE_SOFT, "Stok rendah", d.low_stock, Some(db::StockFilter::Low)),
                (RED, RED_SOFT, "Stok habis", d.empty_stock, Some(db::StockFilter::Empty)),
                (ACCENT_INK, PRIMARY_SOFT, "Stok hampir rendah", d.near_low, None),
                (ORANGE, ORANGE_SOFT, &*format!("Kedaluwarsa ≤ {} hari", db::EXPIRY_WARN_DAYS), d.expiring, None),
            ];
            for (fg, bg, label, n, filter) in rows {
                let right = if filter.is_some() { format!("{} barang  >", n) } else { format!("{} barang", n) };
                if list_row_icon(ui, Some((Icon::Dot, fg, bg)), label, "", &right, if n > 0 { TEXT } else { MUTED }).clicked() {
                    if let Some(f) = filter {
                        app.products.filter = Some(f);
                        app.tab(Screen::Products);
                    }
                }
            }
        });
        section_title(ui, "Ringkasan");
        ui.horizontal(|ui| {
            stat_tile(ui, half, "Jenis Barang", &format!("{} (tanpa batas)", d.products), TEXT);
            stat_tile(ui, half, "Piutang", &app.money(d.receivable), if d.receivable > 0 { RED } else { TEXT });
        });
        if app.can("view_cost") {
            ui.horizontal(|ui| {
                stat_tile(ui, w, "Nilai stok (harga modal)", &app.money(d.stock_value), TEXT);
            });
        }
        section_title(ui, "Transaksi terakhir");
        let (from, to) = (0, db::now_ms() + 86_400_000);
        let recent = app.db.list_tx(None, from, to, "", false, 5);
        if recent.is_empty() {
            muted(ui, "Belum ada transaksi");
        }
        if !recent.is_empty() {
            list_group(ui, |ui| {
                for r in recent {
                    let title = if r.summary.is_empty() { r.kind.label().to_string() } else { r.summary.clone() };
                    let sub = format!("{} · {}", r.kind.label(), db::fmt_dt(r.ts));
                    let right = if r.kind == TxKind::Audit { format!("±{}", db::fmt_qty(r.qty)) } else { app.money(r.total) };
                    if list_row_icon(ui, Some(kind_icon(r.kind)), &title, &sub, &right, TEXT).clicked() {
                        app.go(Screen::TxDetail(r.id.clone()));
                    }
                }
            });
        }
        ui.add_space(12.0);
        });
    });
}

pub fn more_menu(app: &mut BakadaiApp, ui: &mut Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let staff = app.db.staff();
        let me = staff.iter().find(|s| s.id == app.staff_id).cloned().unwrap_or_default();
        card(ui, |ui| {
            ui.label(RichText::new(&app.business_name).family(medium()).size(17.0));
            muted(ui, format!("Masuk sebagai {} ({})", me.name, me.role));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Ganti staff:").size(13.0).color(MUTED));
                for s in staff.iter().filter(|s| s.active) {
                    if chip(ui, s.id == app.staff_id, &s.name).clicked() {
                        let id = s.id.clone();
                        app.switch_staff(&id);
                    }
                }
            });
        });
        // Dikelompokkan seperti desain v2; grup tanpa menu yang diizinkan tidak ditampilkan.
        let groups: Vec<(&str, Vec<(Icon, &str, &str, Screen, &str)>)> = vec![
            ("Kontak", vec![
                (Icon::User, "Pembeli", "Kelola data pelanggan bisnis Anda.", Screen::Contacts("buyer"), "contacts"),
                (Icon::Box, "Vendor", "Kelola data pemasok bisnis Anda.", Screen::Contacts("vendor"), "contacts"),
            ]),
            ("Keuangan", vec![
                (Icon::Chart, "Laporan", "Penjualan, pembelian, keuntungan.", Screen::Reports, "reports"),
                (Icon::Pos, "Laporan Piutang", "Daftar pembeli yang belum bayar", Screen::Receivables, "reports"),
                (Icon::Out, "Pengeluaran", "Catat gaji, sewa, listrik, dan lainnya", Screen::Expenses, "expenses"),
            ]),
            ("Toko", vec![
                (Icon::User, "Staff", "Tanpa batas staff, akses per fitur", Screen::Staff, "staff"),
                (Icon::Filter, "Kategori Barang", "Kelola kategori stok Anda", Screen::Categories("product"), "products"),
                (Icon::Check, "Metode Pembayaran", "Tunai, QRIS, transfer, dll.", Screen::PaymentMethods, "settings"),
                (Icon::Plus, "Biaya Tambahan", "Biaya layanan, ongkir, pajak", Screen::Fees, "settings"),
            ]),
            ("Sistem", vec![
                (Icon::Sliders, "Pengaturan", "Bisnis, struk, printer, stok", Screen::Settings, "settings"),
                (Icon::Sync, "Server & Sinkron", "Hubungkan ke REST API Anda", Screen::Sync, "settings"),
            ]),
        ];
        for (title, items) in groups {
            let items: Vec<_> = items.into_iter().filter(|it| app.can(it.4)).collect();
            if items.is_empty() {
                continue;
            }
            section_title(ui, title);
            list_group(ui, |ui| {
                for (ic, t, s, scr, _) in items {
                    if list_row_icon(ui, Some((ic, ACCENT_INK, PRIMARY_SOFT)), t, s, ">", MUTED).clicked() {
                        match &scr {
                            Screen::Expenses => app.more.e_range = 2,
                            Screen::Reports => app.more.r_range = 0,
                            _ => {}
                        }
                        app.go(scr);
                    }
                }
            });
        }
        ui.add_space(8.0);
        muted(ui, format!("Bakadai Lite {} · data di perangkat ini", env!("CARGO_PKG_VERSION")));
    });
}

// ---- kontak -------------------------------------------------------------------------------

pub fn contacts_screen(app: &mut BakadaiApp, ui: &mut Ui, kind: &'static str) {
    if input_with_button(ui, &mut app.more.c_search, "Cari nama / nomor HP…", "+ Tambah", 0) {
        app.more.contact = Contact { kind: kind.into(), ..Default::default() };
        app.go(Screen::ContactEdit);
    }
    let list = app.db.contacts(kind, &app.more.c_search);
    if list.is_empty() {
        empty_state(ui, if kind == "vendor" { "Belum Ada Vendor" } else { "Belum Ada Pembeli" }, if kind == "vendor" { "Vendor adalah pihak tempat Anda mendapatkan stok." } else { "Tambah pembeli agar piutang mudah dilacak." });
        return;
    }
    let debts: std::collections::HashMap<String, db::Money> = app.db.receivables().into_iter().map(|r| (r.0, r.2)).collect();
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show_rows(ui, 66.0, list.len(), |ui, range| {
        for c in &list[range] {
            let debt = debts.get(&c.id).copied().unwrap_or(0);
            let right = if debt > 0 { app.money(debt) } else { String::new() };
            if list_row(ui, &c.name, &c.phone, &right, RED).clicked() {
                app.more.contact = c.clone();
                app.go(Screen::ContactEdit);
            }
        }
    }));
}

pub fn contact_edit(app: &mut BakadaiApp, ui: &mut Ui) {
    let mut save = false;
    let is_new = app.more.contact.id.is_empty();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let c = &mut app.more.contact;
        card(ui, |ui| {
            field(ui, if c.kind == "vendor" { "Nama Vendor (Wajib)" } else { "Nama Pembeli (Wajib)" }, &mut c.name, "");
            field(ui, "Nomor HP", &mut c.phone, "08…");
            field(ui, "Email", &mut c.email, "");
            field_multiline(ui, "Alamat", &mut c.address, "");
            field_multiline(ui, "Catatan", &mut c.note, "");
        });
        if primary_button(ui, "Simpan").clicked() {
            save = true;
        }
        if !is_new {
            let id = app.more.contact.id.clone();
            let txs = app.db.list_tx(None, 0, db::now_ms() + 86_400_000, "", false, 2000);
            let name = app.more.contact.name.clone();
            let mine: Vec<_> = txs.into_iter().filter(|t| t.contact == name).take(20).collect();
            if !mine.is_empty() {
                section_title(ui, "Riwayat");
                list_group(ui, |ui| {
                    for t in mine {
                        let sub = format!("{} · {}", db::fmt_dt(t.ts), t.pay_status());
                        if list_row_icon(ui, Some(kind_icon(t.kind)), t.kind.label(), &sub, &app.money(t.total), if t.pay_status() == "Lunas" { TEXT } else { RED }).clicked() {
                            app.go(Screen::TxDetail(t.id.clone()));
                        }
                    }
                });
            }
            if danger_button(ui, "Hapus").clicked() {
                app.confirm = Some(("Hapus kontak ini? Transaksi lama tetap tersimpan.".into(), Confirm::DeleteContact(id)));
            }
        }
    });
    if save {
        match app.db.save_contact(&app.more.contact) {
            Ok(_) => {
                app.ok(if app.more.contact.kind == "vendor" { "Data Vendor Berhasil Disimpan" } else { "Pembeli berhasil disimpan" });
                app.changed();
                app.back();
            }
            Err(e) => app.err(e),
        }
    }
}

pub fn receivables_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    let rows = app.db.receivables();
    let total: db::Money = rows.iter().map(|r| r.2).sum();
    card(ui, |ui| {
        kv_color(ui, "Total Piutang", &app.money(total), if total > 0 { RED } else { TEXT });
    });
    if rows.is_empty() {
        empty_state(ui, "Tidak ada piutang", "Semua transaksi sudah lunas.");
        return;
    }
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        for (_, name, amt, n) in rows {
            if list_row(ui, &name, &format!("Ada {} transaksi belum lunas.", n), &app.money(amt), RED).clicked() {
                app.stock.h_unpaid = true;
                app.stock.h_range = 5;
                app.stock.h_search = if name == "Tanpa nama" { String::new() } else { name.clone() };
                app.tab(Screen::History);
            }
        }
    }));
}

// ---- pengeluaran ----------------------------------------------------------------------------

pub fn expenses_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("erange").selected_text(super::stock::RANGES[app.more.e_range]).show_ui(ui, |ui| {
            for (i, r) in super::stock::RANGES.iter().enumerate() {
                ui.selectable_value(&mut app.more.e_range, i, *r);
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if small_button(ui, "+ Tambah").clicked() {
                app.more.expense = Expense { include_profit: true, ts: db::now_ms(), ..Default::default() };
                app.more.expense_amount.clear();
                app.more.expense_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                app.go(Screen::ExpenseEdit);
            }
        });
    });
    let (from, to) = super::stock::range_bounds(app.more.e_range);
    let list = app.db.expenses(from, to);
    let total: db::Money = list.iter().map(|x| x.amount).sum();
    card(ui, |ui| kv_color(ui, "Total Pengeluaran", &app.money(total), RED));
    if list.is_empty() {
        empty_state(ui, "Belum ada pengeluaran yang tercatat", "Catat gaji karyawan hingga tagihan listrik.");
        return;
    }
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        for x in list {
            let sub = format!("{} · {}{}", db::fmt_date(x.ts), x.category, if x.include_profit { "" } else { " · tidak termasuk profit" });
            let title = if x.note.is_empty() { x.category.clone() } else { x.note.clone() };
            if list_row(ui, &title, &sub, &app.money(x.amount), RED).clicked() {
                app.more.expense_amount = money_str(x.amount);
                app.more.expense_date = chrono::DateTime::from_timestamp_millis(x.ts)
                    .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                app.more.expense = x;
                app.go(Screen::ExpenseEdit);
            }
        }
    }));
    ui.add_space(4.0);
}

pub fn expense_edit(app: &mut BakadaiApp, ui: &mut Ui) {
    let cats = app.db.categories("expense");
    let mut save = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let x = &mut app.more.expense;
        card(ui, |ui| {
            num_field(ui, "Nominal", &mut app.more.expense_amount, "0");
            field(ui, "Tanggal (TTTT-BB-HH)", &mut app.more.expense_date, "");
            ui.label(RichText::new("Kategori Pengeluaran").size(13.0).color(MUTED));
            let sel = x.category_id.as_ref().and_then(|id| cats.iter().find(|c| &c.id == id)).map(|c| c.name.clone()).unwrap_or_else(|| "Tanpa Kategori".into());
            egui::ComboBox::from_id_salt("ecat").width(ui.available_width() - 30.0).selected_text(sel).show_ui(ui, |ui| {
                ui.selectable_value(&mut x.category_id, None, "Tanpa Kategori");
                for c in &cats {
                    ui.selectable_value(&mut x.category_id, Some(c.id.clone()), &c.name);
                }
            });
            if ui.link("Kelola kategori pengeluaran").clicked() {
                app.nav.push(Screen::Categories("expense"));
            }
            field_multiline(ui, "Catatan", &mut x.note, "");
            toggle(ui, &mut x.include_profit, "Termasuk ke profit");
            muted(ui, "Jika aktif, biaya ini akan dihitung ke laporan profit");
        });
        if primary_button(ui, "Simpan").clicked() {
            save = true;
        }
        if !app.more.expense.id.is_empty() && danger_button(ui, "Hapus Pengeluaran").clicked() {
            app.confirm = Some(("Hapus pengeluaran ini?".into(), Confirm::DeleteExpense(app.more.expense.id.clone())));
        }
    });
    if save {
        use chrono::TimeZone;
        let ts = chrono::NaiveDate::parse_from_str(app.more.expense_date.trim(), "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(12, 0, 0))
            .and_then(|d| chrono::Local.from_local_datetime(&d).earliest())
            .map(|d| d.timestamp_millis());
        let Some(ts) = ts else {
            app.err("Format tanggal salah. Contoh: 2026-11-17");
            return;
        };
        app.more.expense.ts = ts;
        app.more.expense.amount = db::money_from_f64(parse_money(&app.more.expense_amount));
        match app.db.save_expense(&app.more.expense) {
            Ok(_) => {
                app.ok("Pengeluaran berhasil disimpan");
                app.changed();
                app.back();
            }
            Err(e) => app.err(e),
        }
    }
}

// ---- staff ----------------------------------------------------------------------------------

pub fn staff_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    let list = app.db.staff();
    card(ui, |ui| {
        ui.label(RichText::new(format!("{} staff (termasuk pemilik)", list.len())).family(medium()));
        muted(ui, "Tanpa Batas Staff — tambah sebanyak yang Anda perlukan. Masing-masing staff dapat diberikan akses berbeda.");
    });
    if primary_button(ui, "Tambah Staff").clicked() {
        app.more.staff = Staff { role: "Kasir".into(), permissions: db::default_permissions("Kasir"), active: true, ..Default::default() };
        app.go(Screen::StaffEdit);
    }
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        for s in list {
            let sub = format!("{} · {} akses{}", s.role, s.permissions.len(), if s.active { "" } else { " · nonaktif" });
            if list_row(ui, &s.name, &sub, if s.id == app.staff_id { "Anda" } else { ">" }, if s.active { MUTED } else { RED }).clicked() {
                app.more.staff = s.clone();
                app.go(Screen::StaffEdit);
            }
        }
    }));
}

pub fn staff_edit(app: &mut BakadaiApp, ui: &mut Ui) {
    let mut save = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let s = &mut app.more.staff;
        let is_owner = s.role == "Pemilik" && !s.id.is_empty();
        card(ui, |ui| {
            field(ui, "Nama Staff", &mut s.name, "");
            field(ui, "Nomor HP", &mut s.phone, "");
            field(ui, "Email", &mut s.email, "");
            ui.label(RichText::new("Peran").size(13.0).color(MUTED));
            ui.add_enabled_ui(!is_owner, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for r in db::ROLES.iter().filter(|r| **r != "Pemilik") {
                        if chip(ui, s.role == *r, *r).clicked() {
                            s.role = r.to_string();
                            s.permissions = db::default_permissions(r);
                        }
                    }
                });
            });
            toggle(ui, &mut s.active, "Aktif");
        });
        card(ui, |ui| {
            ui.label(RichText::new("Akses").family(medium()));
            muted(ui, "Atur fitur yang dapat diakses staff.");
            ui.add_enabled_ui(!is_owner, |ui| {
                for (key, label) in db::PERMISSIONS {
                    let mut on = s.permissions.iter().any(|p| p == key);
                    if toggle(ui, &mut on, label).changed() {
                        if on {
                            s.permissions.push(key.to_string());
                        } else {
                            s.permissions.retain(|p| p != key);
                        }
                    }
                }
            });
        });
        if primary_button(ui, "Simpan Perubahan").clicked() {
            save = true;
        }
        if !app.more.staff.id.is_empty() && app.more.staff.role != "Pemilik" && danger_button(ui, "Hapus Staff").clicked() {
            app.confirm = Some((format!("Hapus {}?", app.more.staff.name), Confirm::DeleteStaff(app.more.staff.id.clone())));
        }
    });
    if save {
        match app.db.save_staff(&app.more.staff) {
            Ok(id) => {
                app.ok("Perubahan staff berhasil tersimpan");
                if id == app.staff_id {
                    app.perms = app.more.staff.permissions.clone();
                }
                app.changed();
                app.back();
            }
            Err(e) => app.err(e),
        }
    }
}

// ---- laporan --------------------------------------------------------------------------------

pub fn reports_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    egui::ComboBox::from_id_salt("rrange").selected_text(super::stock::RANGES[app.more.r_range]).show_ui(ui, |ui| {
        for (i, r) in super::stock::RANGES.iter().enumerate() {
            ui.selectable_value(&mut app.more.r_range, i, *r);
        }
    });
    let (from, to) = super::stock::range_bounds(app.more.r_range);
    let key = format!("{}", app.more.r_range);
    if app.more.r_key != key || app.more.r_rev != app.rev {
        app.more.report = app.db.report(from, to);
        app.more.r_key = key;
        app.more.r_rev = app.rev;
    }
    let r = app.more.report.clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let w = ui.available_width();
        let half = (w - 8.0) / 2.0;
        ui.horizontal(|ui| {
            stat_tile(ui, half, "Omset", &app.money(r.sales - r.returns), PRIMARY);
            stat_tile(ui, half, "Laba bersih", &app.money(r.net_profit), if r.net_profit < 0 { RED } else { GREEN });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Grafik penjualan").family(medium()));
            bar_chart(ui, &r.daily, PRIMARY);
        });
        card(ui, |ui| {
            ui.label(RichText::new("Laporan Keuangan").family(medium()));
            kv(ui, "Penjualan (setelah diskon)", &app.money(r.sales));
            kv(ui, "Diskon diberikan", &app.money(r.discount));
            kv(ui, "Retur penjualan", &format!("-{}", app.money(r.returns)));
            kv(ui, "Biaya tambahan (masuk profit)", &app.money(r.fee_income));
            kv(ui, "Harga modal terjual", &format!("-{}", app.money(r.hpp)));
            kv(ui, "Kerugian barang rusak (retur)", &app.money(r.damaged_loss));
            ui.separator();
            kv_color(ui, "Laba kotor", &app.money(r.gross_profit), if r.gross_profit < 0 { RED } else { GREEN });
            kv(ui, "Pengeluaran", &format!("-{}", app.money(r.expenses)));
            kv_color(ui, "Laba bersih", &app.money(r.net_profit), if r.net_profit < 0 { RED } else { GREEN });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Laporan Stok").family(medium()));
            kv(ui, "Jumlah transaksi penjualan", &r.tx_count.to_string());
            kv(ui, "Total stok masuk", &db::fmt_qty(r.qty_in));
            kv(ui, "Total stok keluar", &db::fmt_qty(r.qty_out));
            kv(ui, "Pembelian (stok masuk)", &app.money(r.purchases));
            kv(ui, "Aset stok (harga modal)", &app.money(r.stock_value_cost));
            kv(ui, "Aset stok (harga jual)", &app.money(r.stock_value_sell));
            kv_color(ui, "Piutang", &app.money(r.receivable), if r.receivable > 0 { RED } else { TEXT });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Terlaris").family(medium()));
            if r.best_sellers.is_empty() {
                muted(ui, "Tidak ditemukan data");
            }
            for (i, (name, q, v)) in r.best_sellers.iter().enumerate() {
                kv(ui, &format!("{}. {} ({})", i + 1, name, db::fmt_qty(*q)), &app.money(*v));
            }
        });
        if !r.top_buyers.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("Siapa pembeli paling banyak?").family(medium()));
                for (name, v) in &r.top_buyers {
                    kv(ui, name, &app.money(*v));
                }
            });
        }
        card(ui, |ui| {
            ui.label(RichText::new("Unduh Laporan (CSV, bisa dibuka di Excel)").family(medium()));
            ui.horizontal_wrapped(|ui| {
                for (what, label) in [("laporan", "Ringkasan"), ("riwayat", "Riwayat"), ("barang", "Barang"), ("pengeluaran", "Pengeluaran")] {
                    if small_button(ui, label).clicked() {
                        match app.db.export_csv(what, from, to) {
                            Ok(p) => app.more.export_msg = format!("Dokumen tersimpan: {}", p.display()),
                            Err(e) => app.more.export_msg = format!("Gagal menyimpan dokumen: {}", e),
                        }
                    }
                }
            });
            if !app.more.export_msg.is_empty() {
                ui.label(RichText::new(&app.more.export_msg).size(12.0).color(GREEN));
            }
        });
        ui.add_space(12.0);
    });
}

// ---- pengaturan -----------------------------------------------------------------------------

pub fn settings_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    let m = &mut app.more;
    if !m.settings_loaded {
        m.s_name = app.db.setting("business_name", "Toko Saya");
        m.s_address = app.db.setting("business_address", "");
        m.s_currency = app.db.setting("currency", "Rp");
        m.s_footer = app.db.setting("receipt_footer", "Terima kasih!");
        m.s_paper = app.db.setting("paper_mm", "58");
        m.s_printer = app.db.setting("printer_addr", "");
        m.s_lock = app.db.setting("lock_days", "0");
        m.settings_loaded = true;
    }
    let mut bt_err: Option<String> = None;
    let mut save = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let m = &mut app.more;
        card(ui, |ui| {
            ui.label(RichText::new("Profil Bisnis").family(medium()));
            field(ui, "Nama Bisnis", &mut m.s_name, "");
            field(ui, "Alamat", &mut m.s_address, "");
            field(ui, "Mata Uang", &mut m.s_currency, "Rp");
        });
        card(ui, |ui| {
            ui.label(RichText::new("Stok").family(medium()));
            let mut neg = app.db.setting_bool("allow_negative", false);
            if toggle(ui, &mut neg, "Izinkan Stok Minus").changed() {
                let _ = app.db.set_setting("allow_negative", if neg { "1" } else { "0" });
            }
            let mut avg = app.db.setting_bool("auto_avg_cost", true);
            if toggle(ui, &mut avg, "Auto-sesuaikan harga modal saat stok masuk").changed() {
                let _ = app.db.set_setting("auto_avg_cost", if avg { "1" } else { "0" });
            }
            muted(ui, "Saat aktif, harga modal dihitung ulang sebagai rata-rata tertimbang setiap kali stok masuk.");
            num_field(ui, "Kunci transaksi lebih lama dari (hari, 0 = tidak dikunci)", &mut m.s_lock, "0");
        });
        card(ui, |ui| {
            ui.label(RichText::new("Struk & Printer").family(medium()));
            ui.label(RichText::new("Ukuran Kertas").size(13.0).color(MUTED));
            ui.horizontal(|ui| {
                if chip(ui, m.s_paper == "58", "58 mm").clicked() {
                    m.s_paper = "58".into();
                }
                if chip(ui, m.s_paper == "80", "80 mm").clicked() {
                    m.s_paper = "80".into();
                }
            });
            field(ui, "Teks footer struk", &mut m.s_footer, "Tambahkan teks footer");
            field(ui, "Alamat printer (IP:port jaringan, atau alamat Bluetooth)", &mut m.s_printer, "192.168.1.50:9100");
            if secondary_button(ui, "Pilih printer Bluetooth").clicked() {
                match crate::bt::paired() {
                    Ok(l) if l.is_empty() => bt_err = Some("Belum ada printer yang di-pair. Pair dulu di Pengaturan Bluetooth HP".into()),
                    Ok(l) => m.bt_list = l,
                    Err(e) => bt_err = Some(e),
                }
            }
            for (name, addr) in &m.bt_list {
                if chip(ui, m.s_printer == *addr, format!("{name}  ·  {addr}")).clicked() {
                    m.s_printer = addr.clone();
                }
            }
            let mut auto = app.db.setting_bool("auto_print", false);
            if toggle(ui, &mut auto, "Auto-print Struk setelah bayar").changed() {
                let _ = app.db.set_setting("auto_print", if auto { "1" } else { "0" });
            }
            let mut drawer = app.db.setting_bool("open_drawer", false);
            if toggle(ui, &mut drawer, "Buka Laci Kasir Otomatis").changed() {
                let _ = app.db.set_setting("open_drawer", if drawer { "1" } else { "0" });
            }
        });
        if primary_button(ui, "Simpan Perubahan").clicked() {
            save = true;
        }
        ui.add_space(12.0);
    });
    if let Some(e) = bt_err {
        app.err(e);
    }
    if save {
        let m = &app.more;
        let pairs = [
            ("business_name", m.s_name.clone()),
            ("business_address", m.s_address.clone()),
            ("currency", if m.s_currency.trim().is_empty() { "Rp".into() } else { m.s_currency.trim().to_string() }),
            ("receipt_footer", m.s_footer.clone()),
            ("paper_mm", m.s_paper.clone()),
            ("printer_addr", m.s_printer.trim().to_string()),
            ("lock_days", m.s_lock.trim().parse::<i64>().unwrap_or(0).max(0).to_string()),
        ];
        for (k, v) in pairs {
            let _ = app.db.set_setting(k, &v);
        }
        app.business_name = app.db.setting("business_name", "Toko Saya");
        app.currency = app.db.setting("currency", "Rp");
        app.ok("Pengaturan tersimpan");
        app.changed();
    }
}

pub fn categories_screen(app: &mut BakadaiApp, ui: &mut Ui, kind: &'static str) {
    let label = if app.more.cat_edit.is_some() { "Simpan" } else { "+ Tambah" };
    {
        if input_with_button(ui, &mut app.more.cat_name, "Nama Kategori", label, 0) {
            let id = app.more.cat_edit.clone();
            match app.db.save_category(id.as_deref(), kind, &app.more.cat_name) {
                Ok(_) => {
                    app.ok(if id.is_some() { "Kategori berhasil diperbarui" } else { "Kategori berhasil ditambahkan" });
                    app.more.cat_name.clear();
                    app.more.cat_edit = None;
                    app.changed();
                }
                Err(e) => app.err(e),
            }
        }
    }
    let list = app.db.categories(kind);
    if list.is_empty() {
        empty_state(ui, "Belum Ada Kategori", "Tambahkan kategori untuk memudahkan mengatur barang.");
    }
    for c in list {
        ui.horizontal(|ui| {
            let w = ui.available_width() - 80.0;
            ui.allocate_ui(egui::vec2(w, 46.0), |ui| {
                if list_row(ui, &c.name, "", "ubah", PRIMARY).clicked() {
                    app.more.cat_name = c.name.clone();
                    app.more.cat_edit = Some(c.id.clone());
                }
            });
            if ui.button("Hapus").clicked() {
                app.confirm = Some((format!("Hapus kategori \"{}\"? Barangnya menjadi Tanpa Kategori.", c.name), Confirm::DeleteCategory(c.id.clone())));
            }
        });
    }
}

pub fn payment_methods_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    if input_with_button(ui, &mut app.more.pm_name, "Nama Metode Pembayaran", "+ Tambah", 0) {
        match app.db.save_payment_method(None, &app.more.pm_name) {
            Ok(_) => {
                app.more.pm_name.clear();
                app.ok("Metode pembayaran ditambahkan");
            }
            Err(e) => app.err(e),
        }
    }
    for m in app.db.payment_methods() {
        ui.horizontal(|ui| {
            let w = ui.available_width() - 80.0;
            ui.allocate_ui(egui::vec2(w, 46.0), |ui| {
                list_row(ui, &m.name, if m.is_default { "Metode pembayaran default" } else { "" }, "", MUTED);
            });
            if !m.is_default && ui.button("Hapus").clicked() {
                match app.db.delete_payment_method(&m.id) {
                    Ok(_) => app.ok("Pembayaran berhasil dihapus"),
                    Err(e) => app.err(e),
                }
            }
        });
    }
}

pub fn fees_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            muted(ui, "Tambahkan biaya tambahan seperti biaya layanan, biaya pengiriman, atau pajak. Tanpa batas jumlah.");
            let f = &mut app.more.fee;
            if f.kind.is_empty() {
                f.kind = "percent".into();
                f.include_profit = true;
            }
            field(ui, "Nama", &mut f.name, "contoh: Biaya Layanan");
            let mut k = f.kind.clone();
            ui.horizontal(|ui| {
                if chip(ui, k == "percent", "Persentase").clicked() {
                    k = "percent".into();
                }
                if chip(ui, k == "fixed", "Nominal").clicked() {
                    k = "fixed".into();
                }
            });
            f.kind = k;
            num_field(ui, if f.kind == "percent" { "Nilai (%)" } else { "Nominal" }, &mut app.more.fee_value, "0");
            toggle(ui, &mut app.more.fee.include_profit, "Termasuk profit");
            if primary_button(ui, if app.more.fee.id.is_empty() { "Tambah Biaya Tambahan" } else { "Ubah Biaya Tambahan" }).clicked() {
                let mut f = app.more.fee.clone();
                f.value = parse_qty(&app.more.fee_value);
                match app.db.save_fee_template(&f) {
                    Ok(_) => {
                        app.ok("Biaya tambahan tersimpan");
                        app.more.fee = FeeTemplate::default();
                        app.more.fee_value.clear();
                    }
                    Err(e) => app.err(e),
                }
            }
        });
        let list = app.db.fee_templates();
        if list.is_empty() {
            empty_state(ui, "Belum Ada Biaya Tambahan", "Klik Tambah untuk memulai.");
        }
        for f in list {
            let v = if f.kind == "percent" { format!("{}%", f.value) } else { app.money(db::money_from_f64(f.value)) };
            ui.horizontal(|ui| {
                let w = ui.available_width() - 80.0;
                ui.allocate_ui(egui::vec2(w, 58.0), |ui| {
                    if list_row(ui, &f.name, if f.include_profit { "Termasuk profit" } else { "Tidak termasuk profit" }, &v, PRIMARY).clicked() {
                        app.more.fee_value = format!("{}", f.value);
                        app.more.fee = f.clone();
                    }
                });
                if ui.button("Hapus").clicked() {
                    let _ = app.db.delete_fee_template(&f.id);
                    app.ok("Biaya tambahan dihapus");
                }
            });
        }
    });
}

// ---- server & sinkron -----------------------------------------------------------------------

pub fn sync_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    if app.more.s_api.is_empty() {
        app.more.s_api = app.db.setting("api_base_url", "");
    }
    let busy = app.sync_rx.is_some();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.label(RichText::new("Offline-first").family(medium()));
            muted(ui, "Semua data tersimpan di HP dan aplikasi tetap jalan tanpa internet. Saat terhubung, perubahan dikirim ke server REST API Anda (kontrak di API_CONTRACT.md).");
            kv(ui, "Perubahan menunggu dikirim", &app.db.outbox_count().to_string());
            let last = app.db.setting("last_sync", "");
            kv(ui, "Sinkron terakhir", &last.parse::<i64>().map(db::fmt_dt).unwrap_or_else(|_| "belum pernah".into()));
        });
        card(ui, |ui| {
            field(ui, "Alamat server API", &mut app.more.s_api, "https://api.tokosaya.com");
            ui.columns(2, |c| {
                if secondary_button(&mut c[0], "Simpan").clicked() {
                    let _ = app.db.set_setting("api_base_url", app.more.s_api.trim());
                    app.toast = Some(("Alamat server tersimpan".into(), 2.0, false));
                }
                if secondary_button(&mut c[1], "Tes Koneksi").clicked() && !busy {
                    let _ = app.db.set_setting("api_base_url", app.more.s_api.trim());
                    app.sync_rx = Some(crate::sync::health(c[1].ctx().clone(), app.sync_config()));
                }
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Masuk ke server").family(medium()));
            field(ui, "Email", &mut app.more.api_email, "");
            ui.label(RichText::new("Kata sandi").size(13.0).color(MUTED));
            ui.add(egui::TextEdit::singleline(&mut app.more.api_password).password(true).desired_width(f32::INFINITY));
            let has_token = !app.db.setting("api_token", "").is_empty();
            muted(ui, if has_token { "Status: sudah masuk (token tersimpan)" } else { "Status: belum masuk" });
            if secondary_button(ui, "Masuk").clicked() && !busy {
                let _ = app.db.set_setting("api_base_url", app.more.s_api.trim());
                app.sync_rx = Some(crate::sync::login(ui.ctx().clone(), app.sync_config(), app.more.api_email.clone(), app.more.api_password.clone()));
                app.more.api_password.clear();
            }
        });
        if primary_button_enabled(ui, !busy, if busy { "Sedang sinkron…" } else { "Sinkron Sekarang" }).clicked() {
            let _ = app.db.set_setting("api_base_url", app.more.s_api.trim());
            app.sync_rx = Some(crate::sync::sync_now(ui.ctx().clone(), app.data_dir.clone(), app.sync_config()));
        }
        if !app.sync_status.is_empty() {
            muted(ui, app.sync_status.clone());
        }
    });
}
