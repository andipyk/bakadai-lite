//! Daftar barang (tanpa batas jumlah, digulir virtual), tambah/ubah barang, detail & riwayat barang.

use super::widgets::*;
use super::{Confirm, BakadaiApp, Screen};
use crate::db::{self, Db, PriceTier, Product, ProductRow, ProductSort, StockFilter, TxKind};
use egui::{Align, Layout, RichText, Ui, Vec2};

#[derive(Default)]
pub struct ProductsState {
    rev: u64,
    pub rows: Vec<ProductRow>,
    pub search: String,
    pub category: Option<String>,
    pub filter: Option<StockFilter>,
    pub sort: Option<ProductSort>,
    last_query: String,
    pub form: ProductForm,
    show_filters: bool,
}

#[derive(Default, Clone)]
pub struct ProductForm {
    pub id: String,
    pub code: String,
    pub name: String,
    pub category_id: Option<String>,
    pub unit: String,
    pub cost: String,
    pub sell: String,
    pub initial_stock: String,
    pub min_stock: String,
    pub expiry: String,
    pub location: String,
    pub description: String,
    pub tiers: Vec<(String, String, String, String)>, // id, nama, min qty, harga
    pub photo: Option<String>,
    pub thumb: Option<String>,
    pub photo_src: String,
    pub photo_info: String,
    pub new_category: String,
    pub warned_price: bool,
}

impl ProductsState {
    pub fn open_new(&mut self, db: &Db) {
        self.form = ProductForm { code: db.random_code(), unit: "pcs".into(), ..Default::default() };
    }
    pub fn open_edit(&mut self, p: &Product) {
        self.form = ProductForm {
            id: p.id.clone(),
            code: p.code.clone(),
            name: p.name.clone(),
            category_id: p.category_id.clone(),
            unit: p.unit.clone(),
            cost: money_str(p.cost_price),
            sell: money_str(p.sell_price),
            initial_stock: String::new(),
            min_stock: db::fmt_qty(p.min_stock),
            expiry: p.expiry_date.clone().unwrap_or_default(),
            location: p.location.clone(),
            description: p.description.clone(),
            tiers: p.tiers.iter().map(|t| (t.id.clone(), t.name.clone(), db::fmt_qty(t.min_qty), money_str(t.price))).collect(),
            photo: p.photo.clone(),
            thumb: p.thumb.clone(),
            ..Default::default()
        };
    }

    fn refresh(&mut self, app_rev: u64, db: &Db) {
        let q = format!(
            "{}|{:?}|{:?}|{:?}",
            self.search,
            self.category,
            self.filter.unwrap_or(StockFilter::All),
            self.sort.unwrap_or(ProductSort::NameAsc)
        );
        if self.rev != app_rev || q != self.last_query {
            self.rows = db.list_products(
                &self.search,
                self.category.as_deref(),
                self.filter.unwrap_or(StockFilter::All),
                self.sort.unwrap_or(ProductSort::NameAsc),
            );
            self.rev = app_rev;
            self.last_query = q;
        }
    }
}

/// Gambar kecil barang: thumbnail terkompres kalau ada, kalau tidak kotak inisial.
pub fn product_thumb(app: &mut BakadaiApp, ui: &mut Ui, name: &str, thumb: Option<&String>, size: f32) -> egui::Response {
    if let Some(t) = thumb {
        let ctx = ui.ctx().clone();
        if let Some(tex) = app.thumbs.get(&ctx, t) {
            return ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::splat(size)).corner_radius(10).sense(egui::Sense::click()));
        }
    }
    placeholder_tile(ui, name, size)
}

pub fn list_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    let rev = app.rev;
    app.products.refresh(rev, &app.db);
    let can_edit = app.can("products");
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 38.0), Layout::right_to_left(Align::Center), |ui| {
        if can_edit && small_button(ui, "+ Baru").clicked() {
            app.products.open_new(&app.db);
            app.go(Screen::ProductEdit);
        }
        if icon_button(ui, Icon::Filter, PRIMARY, PRIMARY_SOFT).on_hover_text("Urutkan dan Filter Barang").clicked() {
            app.products.show_filters = !app.products.show_filters;
        }
        if icon_button(ui, Icon::Scan, PRIMARY, PRIMARY_SOFT).on_hover_text("Scan barcode pakai kamera").clicked() {
            app.open_scanner();
        }
        ui.add(
            egui::TextEdit::singleline(&mut app.products.search)
                .hint_text("Cari nama / kode barang…")
                .desired_width(ui.available_width())
                .margin(egui::Margin::symmetric(10, 9)),
        );
    });
    // Scan di daftar barang: barang yang ada dibuka, kode baru langsung jadi barang baru dengan kode terisi.
    if let Some(code) = app.take_scan() {
        if let Some(p) = app.db.product_by_code(&code) {
            app.go(Screen::ProductDetail(p.id));
        } else if can_edit {
            app.products.open_new(&app.db);
            app.products.form.code = code;
            app.go(Screen::ProductEdit);
            app.ok("Barang baru: kode diisi dari hasil scan");
        } else {
            app.err(format!("Kode Barang tidak tersedia: {code}"));
        }
    }
    if app.products.show_filters {
        card(ui, |ui| {
            let cats = app.db.categories("product");
            ui.horizontal_wrapped(|ui| {
                let sel = app.products.category.clone();
                if chip(ui, sel.is_none(), "Semua Kategori").clicked() {
                    app.products.category = None;
                }
                for c in &cats {
                    if chip(ui, sel.as_deref() == Some(c.id.as_str()), &c.name).clicked() {
                        app.products.category = Some(c.id.clone());
                    }
                }
                if chip(ui, sel.as_deref() == Some("__none__"), "Tanpa Kategori").clicked() {
                    app.products.category = Some("__none__".into());
                }
            });
            ui.separator();
            let mut f = app.products.filter.unwrap_or(StockFilter::All);
            segmented(ui, &mut f, &[(StockFilter::All, "Semua Stok"), (StockFilter::Low, "Stok Rendah"), (StockFilter::Empty, "Habis")]);
            app.products.filter = Some(f);
            let mut s = app.products.sort.unwrap_or(ProductSort::NameAsc);
            egui::ComboBox::from_id_salt("sort").selected_text(s.label()).show_ui(ui, |ui| {
                for o in ProductSort::ALL {
                    ui.selectable_value(&mut s, o, o.label());
                }
            });
            app.products.sort = Some(s);
        });
    }
    let n = app.products.rows.len();
    let total_qty: f64 = app.products.rows.iter().map(|r| r.stock.max(0.0)).sum();
    // Aksi stok sebagai chip 44 px (dulu tautan kecil, sulit diketuk).
    ui.horizontal(|ui| {
        if app.can("stock_in") && chip(ui, false, "Stok masuk").clicked() {
            app.stock.open(TxKind::In, None);
            app.go(Screen::StockForm(TxKind::In));
        }
        if app.can("stock_out") && chip(ui, false, "Stok keluar").clicked() {
            app.stock.open(TxKind::Out, None);
            app.go(Screen::StockForm(TxKind::Out));
        }
        if app.can("audit") && chip(ui, false, "Audit").clicked() {
            app.stock.open(TxKind::Audit, None);
            app.go(Screen::StockForm(TxKind::Audit));
        }
    });
    muted(ui, format!("{} barang · total stok {}", n, db::fmt_qty(total_qty)));
    if n == 0 {
        empty_state(ui, "Belum Ada Barang", "Tambah barang pertama untuk mulai memantau stok.");
        return;
    }
    let row_h = 64.0;
    let show_cost = app.can("view_cost");
    list_group(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, true]).show_rows(ui, row_h, n, |ui, range| {
        for i in range {
            let r = app.products.rows[i].clone();
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), egui::Sense::click());
            ui.painter().rect_filled(rect, 20, if resp.hovered() { PRIMARY_SOFT } else { CARD });
            let thumb_rect = egui::Rect::from_min_size(rect.min + Vec2::new(8.0, 9.0), Vec2::splat(46.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(thumb_rect));
            product_thumb(app, &mut child, &r.name, r.thumb.as_ref(), 46.0);
            let p = ui.painter().clone();
            let st = r.stock_state();
            let col = match st {
                db::StockState::Empty => RED,
                db::StockState::Low => ORANGE,
                _ => TEXT,
            };
            let right_x = rect.right() - 10.0;
            let qty_rect = p.text(
                egui::pos2(right_x, rect.top() + 20.0),
                egui::Align2::RIGHT_CENTER,
                format!("{} {}", db::fmt_qty(r.stock), r.unit),
                egui::FontId::proportional(15.0),
                col,
            );
            let (pill_txt, pill_fg, pill_bg) = match st {
                db::StockState::Empty => ("Habis", RED, RED_SOFT),
                db::StockState::Low => ("Stok Rendah", ORANGE, ORANGE_SOFT),
                db::StockState::NearLow => ("Hampir Rendah", ACCENT_INK, PRIMARY_SOFT),
                db::StockState::Ok => ("", TEXT, CARD),
            };
            let mut left_limit = qty_rect.left();
            if !pill_txt.is_empty() {
                let g = p.layout_no_wrap(pill_txt.to_string(), egui::FontId::proportional(11.5), pill_fg);
                let pr = egui::Rect::from_min_size(egui::pos2(right_x - g.size().x - 14.0, rect.top() + 34.0), g.size() + Vec2::new(14.0, 4.0));
                p.rect_filled(pr, 10, pill_bg);
                p.galley(pr.min + Vec2::new(7.0, 2.0), g, pill_fg);
                left_limit = left_limit.min(pr.left());
            }
            let tx = rect.left() + 64.0;
            let max_w = (left_limit - tx - 8.0).max(40.0);
            let nf = egui::FontId::proportional(15.0);
            p.text(egui::pos2(tx, rect.top() + 11.0), egui::Align2::LEFT_TOP, fit_text(&p, &r.name, &nf, max_w), nf, TEXT);
            let price = if show_cost {
                format!("{} · {} · modal {}", r.code, app.money(r.sell_price), app.money(r.cost_price))
            } else {
                format!("{} · {}", r.code, app.money(r.sell_price))
            };
            let sf = egui::FontId::proportional(12.0);
            p.text(egui::pos2(tx, rect.top() + 36.0), egui::Align2::LEFT_TOP, fit_text(&p, &price, &sf, max_w), sf, MUTED);
            if resp.clicked() {
                app.go(Screen::ProductDetail(r.id.clone()));
            }
        }
    }));
}

pub fn detail_screen(app: &mut BakadaiApp, ui: &mut Ui, id: &str) {
    let Some(p) = app.db.product(id) else {
        empty_state(ui, "Barang tidak ditemukan", "");
        return;
    };
    let cat = p
        .category_id
        .as_ref()
        .and_then(|c| app.db.categories("product").into_iter().find(|x| &x.id == c))
        .map(|c| c.name)
        .unwrap_or_else(|| "Tanpa Kategori".into());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let thumb = p.thumb.clone();
                product_thumb(app, ui, &p.name, thumb.as_ref(), 72.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&p.name).family(medium()).size(18.0));
                    muted(ui, format!("{} · {}", p.code, cat));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Stok {} {}", db::fmt_qty(p.stock), p.unit)).family(medium()));
                        stock_pill(ui, db::stock_state(p.stock, p.min_stock));
                    });
                });
            });
            ui.separator();
            kv(ui, "Harga Jual", &app.money(p.sell_price));
            if app.can("view_cost") {
                kv(ui, "Harga Modal", &app.money(p.cost_price));
                let margin = p.sell_price - p.cost_price;
                kv_color(ui, "Untung per unit", &app.money(margin), if margin < 0 { RED } else { GREEN });
            }
            kv(ui, "Stok Minimal", &db::fmt_qty(p.min_stock));
            if let Some(x) = &p.expiry_date {
                kv(ui, "Tanggal kadaluwarsa", x);
            }
            if !p.location.is_empty() {
                kv(ui, "Lokasi Produk", &p.location);
            }
            for t in &p.tiers {
                kv(ui, &format!("{} (≥ {})", t.name, db::fmt_qty(t.min_qty)), &app.money(t.price));
            }
            if !p.description.is_empty() {
                muted(ui, p.description.clone());
            }
        });
        ui.columns(3, |c| {
            if app.can("stock_in") && secondary_button(&mut c[0], "Masuk").clicked() {
                app.stock.open(TxKind::In, Some(&p));
                app.go(Screen::StockForm(TxKind::In));
            }
            if app.can("stock_out") && secondary_button(&mut c[1], "Keluar").clicked() {
                app.stock.open(TxKind::Out, Some(&p));
                app.go(Screen::StockForm(TxKind::Out));
            }
            if app.can("audit") && secondary_button(&mut c[2], "Audit").clicked() {
                app.stock.open(TxKind::Audit, Some(&p));
                app.go(Screen::StockForm(TxKind::Audit));
            }
        });
        section_title(ui, "Riwayat Barang");
        let hist = app.db.product_history(id, 50);
        if hist.is_empty() {
            muted(ui, "Belum Ada Riwayat");
        }
        for (kind, ts, number, qty, after) in hist {
            let sign = match kind {
                TxKind::In | TxKind::SaleReturn => "+",
                TxKind::Audit => {
                    if qty >= 0.0 {
                        "+"
                    } else {
                        ""
                    }
                }
                _ => "−",
            };
            let col = if sign == "+" { GREEN } else { RED };
            let sub = format!("{} · {}{}", db::fmt_dt(ts), number, after.map(|a| format!(" · sisa {}", db::fmt_qty(a))).unwrap_or_default());
            list_row(ui, kind.label(), &sub, &format!("{}{}", sign, db::fmt_qty(qty.abs())), col);
        }
        ui.add_space(8.0);
        if app.can("products") {
            if primary_button(ui, "Ubah Barang").clicked() {
                app.products.open_edit(&p);
                app.go(Screen::ProductEdit);
            }
            if danger_button(ui, "Hapus Barang").clicked() {
                app.confirm = Some((format!("Hapus \"{}\"? Riwayat tetap tersimpan.", p.name), Confirm::DeleteProduct(p.id.clone())));
            }
        }
    });
}

pub fn edit_screen(app: &mut BakadaiApp, ui: &mut Ui) {
    // Foto yang diseret ke jendela (komputer) langsung dipakai.
    let dropped: Vec<std::path::PathBuf> = ui.ctx().input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
    if let Some(p) = dropped.first() {
        app.products.form.photo_src = p.to_string_lossy().to_string();
        compress_into_form(app);
    }
    let is_new = app.products.form.id.is_empty();
    let cats = app.db.categories("product");
    let mut save = false;
    let mut save_again = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.label(RichText::new("Foto Produk").size(13.0).color(MUTED));
            ui.horizontal(|ui| {
                let name = if app.products.form.name.is_empty() { "Foto".to_string() } else { app.products.form.name.clone() };
                let thumb = app.products.form.thumb.clone();
                product_thumb(app, ui, &name, thumb.as_ref(), 72.0);
                ui.vertical(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.products.form.photo_src)
                            .hint_text("Seret foto ke sini atau tempel path file")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if small_button(ui, "Kompres & Pakai").clicked() {
                            compress_into_form(app);
                        }
                        if app.products.form.photo.is_some() && small_button(ui, "Hapus foto").clicked() {
                            app.products.form.photo = None;
                            app.products.form.thumb = None;
                            app.products.form.photo_info.clear();
                        }
                    });
                    if !app.products.form.photo_info.is_empty() {
                        ui.label(RichText::new(&app.products.form.photo_info).size(12.0).color(GREEN));
                    }
                });
            });
        });
        if let Some(code) = app.take_scan() {
            app.products.form.code = code;
        }
        let f = &mut app.products.form;
        card(ui, |ui| {
            field(ui, "Nama Barang (Wajib)", &mut f.name, "contoh: Kopi Bubuk 200g");
            ui.label(RichText::new("Kode Barang / Barcode (Wajib)").size(13.0).color(MUTED));
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 52.0), Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Scan, TEXT, BG).on_hover_text("Scan barcode pakai kamera").clicked() {
                    app.want_scan = true;
                }
                if small_button(ui, "Acak").clicked() {
                    f.code = app.db.random_code();
                }
                // Kode barang biasanya angka barcode; tombol ABC di papan angka untuk kode berhuruf.
                let r = ui.add(
                    egui::TextEdit::singleline(&mut f.code)
                        .char_limit(db::MAX_CODE_LEN)
                        .desired_width(ui.available_width())
                        .margin(egui::Margin::symmetric(16, 15)),
                );
                mark_numeric(ui, &r, &mut f.code);
            });
            ui.label(RichText::new("Kategori").size(13.0).color(MUTED));
            let sel_name = f
                .category_id
                .as_ref()
                .and_then(|id| cats.iter().find(|c| &c.id == id))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Tanpa Kategori".into());
            egui::ComboBox::from_id_salt("cat").width(ui.available_width() - 30.0).selected_text(sel_name).show_ui(ui, |ui| {
                ui.selectable_value(&mut f.category_id, None, "Tanpa Kategori");
                for c in &cats {
                    ui.selectable_value(&mut f.category_id, Some(c.id.clone()), &c.name);
                }
            });
            if input_with_button(ui, &mut f.new_category, "Kategori baru…", "Tambah", 0) {
                match app.db.save_category(None, "product", &f.new_category) {
                    Ok(id) => {
                        f.category_id = Some(id);
                        f.new_category.clear();
                    }
                    Err(e) => app.toast = Some((e, 3.0, true)),
                }
            }
        });
        card(ui, |ui| {
            ui.columns(2, |c| {
                num_field(&mut c[0], "Harga Modal", &mut f.cost, "0");
                num_field(&mut c[1], "Harga Jual", &mut f.sell, "0");
            });
            let cost = parse_money(&f.cost);
            let sell = parse_money(&f.sell);
            if sell > 0.0 && sell < cost {
                ui.label(RichText::new("Harga Jual < Harga Modal").color(RED).size(12.5));
            }
            ui.columns(2, |c| {
                if is_new {
                    num_field(&mut c[0], "Stok Awal (Wajib)", &mut f.initial_stock, "0");
                } else {
                    c[0].label(RichText::new("Stok saat ini hanya dapat diubah melalui Stok Masuk, Keluar atau Audit.").size(11.5).color(MUTED));
                }
                num_field(&mut c[1], "Stok Minimal", &mut f.min_stock, "0");
            });
            ui.columns(2, |c| {
                field(&mut c[0], "Satuan", &mut f.unit, "pcs");
                field(&mut c[1], "Tanggal kadaluwarsa", &mut f.expiry, "TTTT-BB-HH");
            });
            field(ui, "Lokasi Produk (Tidak Wajib)", &mut f.location, "contoh: Rak A2");
            field_multiline(ui, "Deskripsi", &mut f.description, "");
        });
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Harga Grosir (Multi-tier Pricing)").family(medium()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if small_button(ui, "+ Tambah").clicked() {
                        f.tiers.push((String::new(), String::new(), "10".into(), String::new()));
                    }
                });
            });
            muted(ui, "Harga otomatis dipakai di kasir saat jumlah beli mencapai minimal.");
            let mut remove = None;
            for (i, t) in f.tiers.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 50.0) / 3.0;
                    ui.add(egui::TextEdit::singleline(&mut t.1).hint_text("Nama Harga").desired_width(w));
                    let r = ui.add(egui::TextEdit::singleline(&mut t.2).hint_text("Min qty").desired_width(w * 0.7));
                    mark_numeric(ui, &r, &mut t.2);
                    let r = ui.add(egui::TextEdit::singleline(&mut t.3).hint_text("Harga").desired_width(w * 1.2));
                    mark_numeric(ui, &r, &mut t.3);
                    if icon_button(ui, Icon::Close, RED, RED_SOFT).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                f.tiers.remove(i);
            }
        });
        ui.add_space(4.0);
        if primary_button(ui, "Simpan").clicked() {
            save = true;
        }
        if is_new && secondary_button(ui, "Simpan & Buat Lagi").clicked() {
            save = true;
            save_again = true;
        }
        ui.add_space(20.0);
    });
    if save {
        let f = app.products.form.clone();
        let cost = parse_money(&f.cost);
        let sell = parse_money(&f.sell);
        if sell > 0.0 && sell < cost && !f.warned_price {
            app.products.form.warned_price = true;
            app.err("Harga jual yang Anda masukkan lebih murah daripada harga modal. Tekan Simpan lagi untuk lanjut.");
            return;
        }
        if f.id.is_empty() && f.initial_stock.trim().is_empty() {
            app.err("Mohon isi stok awal");
            return;
        }
        let p = Product {
            id: f.id.clone(),
            code: f.code.clone(),
            name: f.name.clone(),
            category_id: f.category_id.clone(),
            unit: f.unit.clone(),
            cost_price: db::money_from_f64(cost),
            sell_price: db::money_from_f64(sell),
            min_stock: parse_qty(&f.min_stock),
            expiry_date: if f.expiry.trim().is_empty() { None } else { Some(f.expiry.trim().to_string()) },
            location: f.location.clone(),
            description: f.description.clone(),
            photo: f.photo.clone(),
            thumb: f.thumb.clone(),
            tiers: f
                .tiers
                .iter()
                .map(|t| PriceTier { id: t.0.clone(), name: t.1.clone(), min_qty: parse_qty(&t.2), price: db::money_from_f64(parse_money(&t.3)) })
                .collect(),
            ..Default::default()
        };
        if let Some(x) = &p.expiry_date {
            if chrono::NaiveDate::parse_from_str(x, "%Y-%m-%d").is_err() {
                app.err("Format tanggal kadaluwarsa: TTTT-BB-HH (contoh 2026-12-31)");
                return;
            }
        }
        let init = parse_qty(&f.initial_stock);
        match app.db.save_product(&p, Some(init)) {
            Ok(id) => {
                if let Some(ph) = &p.photo {
                    if f.photo_src.trim().len() > 0 {
                        let _ = app.db.queue_photo_upload(&id, ph);
                    }
                }
                app.ok(format!("{} berhasil disimpan", p.name.trim()));
                app.changed();
                if save_again {
                    app.products.open_new(&app.db);
                } else {
                    app.back();
                }
            }
            Err(e) => app.err(e),
        }
    }
}

fn compress_into_form(app: &mut BakadaiApp) {
    let src = app.products.form.photo_src.trim().trim_matches('"').to_string();
    if src.is_empty() {
        app.err("Pilih file foto dulu");
        return;
    }
    let base = format!("p{}", db::new_id());
    match crate::imgproc::compress_photo(std::path::Path::new(&src), &app.db.dir.join("photos"), &base) {
        Ok(c) => {
            let thumb = c.thumb.to_string_lossy().to_string();
            app.thumbs.forget(&thumb);
            app.products.form.photo = Some(c.photo.to_string_lossy().to_string());
            app.products.form.thumb = Some(thumb);
            app.products.form.photo_info = format!(
                "Dikompres {}×{} · {} KB → {} KB (hemat {:.0}%)",
                c.width,
                c.height,
                c.original_bytes / 1024,
                c.photo_bytes.max(1024) / 1024,
                c.saved_percent().max(0.0)
            );
        }
        Err(e) => app.err(e),
    }
}
