//! Penyimpanan lokal (SQLite) + aturan bisnis (stok, harga modal rata-rata, laporan).
//! Semua perubahan dicatat ke tabel `outbox` agar bisa dikirim ke REST API (lihat API_CONTRACT.md).

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Uang disimpan sebagai bilangan bulat dalam satuan 1/100 (sen) agar tidak ada galat pembulatan.
pub type Money = i64;

pub const MAX_CODE_LEN: usize = 25; // "Kode Barang Maksimal 25 Karakter"
pub const MAX_PRICE_DIGITS: usize = 13; // "Harga Jual/Modal Maksimal 13 Digit"
pub const MAX_DISCOUNT_PCT: f64 = 100.0; // "Maksimum diskon adalah 100% dari total harga"
pub const MAX_SURCHARGE_PCT: f64 = 99.99; // "Maksimum surcharge adalah 99.99%"
pub const EXPIRY_WARN_DAYS: i64 = 30;
/// "Stok Hampir Rendah": stok di atas minimal tapi masih ≤ 1,5× minimal (perkiraan, lihat MECHANICS.md).
pub const NEAR_LOW_FACTOR: f64 = 1.5;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// ID unik yang bisa dibuat offline di perangkat (urut waktu + acak), aman untuk sinkron.
pub fn new_id() -> String {
    let t = now_ms() as u64;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mut x = t ^ (nanos << 17) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    format!("{:012x}{:010x}", t & 0xffff_ffff_ffff, x & 0xff_ffff_ffff)
}

pub fn money_from_f64(v: f64) -> Money {
    (v * 100.0).round() as Money
}
pub fn money_to_f64(m: Money) -> f64 {
    m as f64 / 100.0
}

/// Format "Rp 12.500" (titik ribuan, koma desimal bila ada sen).
pub fn fmt_money(m: Money, currency: &str) -> String {
    let neg = m < 0;
    let abs = m.unsigned_abs();
    let whole = abs / 100;
    let cents = abs % 100;
    let s = whole.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if cents > 0 {
        out.push_str(&format!(",{:02}", cents));
    }
    format!("{}{} {}", if neg { "-" } else { "" }, currency, out)
}

pub fn fmt_qty(q: f64) -> String {
    if (q - q.round()).abs() < 1e-9 {
        format!("{}", q.round() as i64)
    } else {
        let s = format!("{:.3}", q);
        s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
    }
}

// ---------------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Product {
    pub id: String,
    pub code: String,
    pub name: String,
    pub category_id: Option<String>,
    pub unit: String,
    pub cost_price: Money,
    pub sell_price: Money,
    pub stock: f64,
    pub min_stock: f64,
    pub expiry_date: Option<String>,
    pub location: String,
    pub description: String,
    pub photo: Option<String>,
    pub thumb: Option<String>,
    pub tiers: Vec<PriceTier>,
}

#[derive(Clone, Debug, Default)]
pub struct PriceTier {
    pub id: String,
    pub name: String,
    pub min_qty: f64,
    pub price: Money,
}

#[derive(Clone, Debug)]
pub struct ProductRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub category_id: Option<String>,
    pub unit: String,
    pub stock: f64,
    pub min_stock: f64,
    pub sell_price: Money,
    pub cost_price: Money,
    pub thumb: Option<String>,
    pub expiry_date: Option<String>,
}

impl ProductRow {
    pub fn stock_state(&self) -> StockState {
        stock_state(self.stock, self.min_stock)
    }
}

pub fn stock_state(stock: f64, min: f64) -> StockState {
    if stock <= 0.0 {
        StockState::Empty
    } else if min > 0.0 && stock <= min {
        StockState::Low
    } else if min > 0.0 && stock <= min * NEAR_LOW_FACTOR {
        StockState::NearLow
    } else {
        StockState::Ok
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StockState {
    Ok,
    NearLow,
    Low,
    Empty,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProductSort {
    NameAsc,
    NameDesc,
    StockAsc,
    StockDesc,
    Newest,
    ExpirySoon,
}

impl ProductSort {
    pub const ALL: [ProductSort; 6] = [
        ProductSort::NameAsc,
        ProductSort::NameDesc,
        ProductSort::StockAsc,
        ProductSort::StockDesc,
        ProductSort::Newest,
        ProductSort::ExpirySoon,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ProductSort::NameAsc => "Abjad A-Z",
            ProductSort::NameDesc => "Abjad Z-A",
            ProductSort::StockAsc => "Stok Rendah ke Tinggi",
            ProductSort::StockDesc => "Stok Tinggi ke Rendah",
            ProductSort::Newest => "Terbaru",
            ProductSort::ExpirySoon => "Kadaluwarsa Terdekat",
        }
    }
    fn sql(self) -> &'static str {
        match self {
            ProductSort::NameAsc => "name COLLATE NOCASE ASC",
            ProductSort::NameDesc => "name COLLATE NOCASE DESC",
            ProductSort::StockAsc => "stock ASC, name COLLATE NOCASE",
            ProductSort::StockDesc => "stock DESC, name COLLATE NOCASE",
            ProductSort::Newest => "updated_at DESC",
            ProductSort::ExpirySoon => "expiry_date IS NULL, expiry_date ASC",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StockFilter {
    All,
    Low,
    Empty,
}

#[derive(Clone, Debug)]
pub struct Category {
    pub id: String,
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Contact {
    pub id: String,
    pub kind: String, // buyer | vendor
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub category_id: Option<String>,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct PaymentMethod {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FeeTemplate {
    pub id: String,
    pub name: String,
    pub kind: String, // percent | fixed
    pub value: f64,
    pub include_profit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxKind {
    In,
    Out,
    Audit,
    SaleReturn,
    PurchaseReturn,
}

impl TxKind {
    pub fn code(self) -> &'static str {
        match self {
            TxKind::In => "in",
            TxKind::Out => "out",
            TxKind::Audit => "audit",
            TxKind::SaleReturn => "sale_return",
            TxKind::PurchaseReturn => "purchase_return",
        }
    }
    pub fn from_code(s: &str) -> TxKind {
        match s {
            "in" => TxKind::In,
            "audit" => TxKind::Audit,
            "sale_return" => TxKind::SaleReturn,
            "purchase_return" => TxKind::PurchaseReturn,
            _ => TxKind::Out,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TxKind::In => "Stok Masuk",
            TxKind::Out => "Stok Keluar",
            TxKind::Audit => "Stok Audit",
            TxKind::SaleReturn => "Retur Penjualan",
            TxKind::PurchaseReturn => "Retur Pembelian",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            TxKind::In => "IN",
            TxKind::Out => "OUT",
            TxKind::Audit => "AUD",
            TxKind::SaleReturn => "RJ",
            TxKind::PurchaseReturn => "RB",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CartLine {
    pub product_id: String,
    pub name: String,
    pub qty: f64,
    /// Harga per unit (jual untuk keluar, modal untuk masuk). Untuk audit, `qty` = stok hasil hitung.
    pub price: Money,
    pub tier: Option<String>,
    /// Untuk retur penjualan: jumlah yang rusak (tidak kembali ke stok).
    pub damaged: f64,
}

#[derive(Clone, Debug, Default)]
pub struct FeeLine {
    pub name: String,
    pub kind: String,
    pub value: f64,
    pub include_profit: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NewTx {
    pub kind: Option<TxKind>,
    pub ts: i64,
    pub contact_id: Option<String>,
    pub staff_id: Option<String>,
    pub lines: Vec<CartLine>,
    pub discount_pct: bool,
    pub discount_value: f64,
    pub fees: Vec<FeeLine>,
    pub paid: Money,
    pub payment_method_id: Option<String>,
    pub ref_id: Option<String>,
    pub refund: Money,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub subtotal: Money,
    pub discount: Money,
    pub fees: Money,
    pub fees_profit: Money,
    pub total: Money,
}

/// Subtotal → diskon → biaya tambahan → total.
/// Diskon persen dibatasi 100%, diskon nominal tidak boleh melebihi subtotal.
/// Biaya persen dihitung dari (subtotal − diskon) dan dibatasi 99,99%.
pub fn compute_totals(lines: &[CartLine], discount_pct: bool, discount_value: f64, fees: &[FeeLine]) -> Totals {
    let subtotal: Money = lines.iter().map(|l| (l.price as f64 * l.qty).round() as Money).sum();
    let discount = if discount_pct {
        let p = discount_value.clamp(0.0, MAX_DISCOUNT_PCT);
        (subtotal as f64 * p / 100.0).round() as Money
    } else {
        money_from_f64(discount_value.max(0.0)).min(subtotal)
    };
    let base = subtotal - discount;
    let mut fee_sum = 0;
    let mut fee_profit = 0;
    for f in fees {
        let amt = fee_amount(f, base);
        fee_sum += amt;
        if f.include_profit {
            fee_profit += amt;
        }
    }
    Totals { subtotal, discount, fees: fee_sum, fees_profit: fee_profit, total: base + fee_sum }
}

pub fn fee_amount(f: &FeeLine, base: Money) -> Money {
    if f.kind == "percent" {
        (base as f64 * f.value.clamp(0.0, MAX_SURCHARGE_PCT) / 100.0).round() as Money
    } else {
        money_from_f64(f.value.max(0.0))
    }
}

/// Harga modal rata-rata tertimbang saat stok masuk:
/// ((stok lama × modal lama) + (jumlah masuk × modal masuk)) ÷ (stok lama + jumlah masuk).
/// Bila stok lama ≤ 0, harga modal baru = modal masuk.
pub fn weighted_cost(old_stock: f64, old_cost: Money, in_qty: f64, in_cost: Money) -> Money {
    if old_stock <= 0.0 || old_stock + in_qty <= 0.0 {
        return in_cost;
    }
    ((old_stock * old_cost as f64 + in_qty * in_cost as f64) / (old_stock + in_qty)).round() as Money
}

/// Harga grosir: tingkat dengan jumlah minimal terbesar yang ≤ qty; kalau tidak ada, harga ecer.
pub fn tier_price(sell_price: Money, tiers: &[PriceTier], qty: f64) -> (Money, Option<String>) {
    let mut best: Option<&PriceTier> = None;
    for t in tiers {
        if qty >= t.min_qty && best.map(|b| t.min_qty > b.min_qty).unwrap_or(true) {
            best = Some(t);
        }
    }
    match best {
        Some(t) => (t.price, Some(t.name.clone())),
        None => (sell_price, None),
    }
}

#[derive(Clone, Debug)]
pub struct TxRow {
    pub id: String,
    pub kind: TxKind,
    pub number: String,
    pub ts: i64,
    pub contact: String,
    pub staff: String,
    pub total: Money,
    pub paid: Money,
    pub items: i64,
    pub qty: f64,
    pub summary: String,
}

impl TxRow {
    pub fn pay_status(&self) -> &'static str {
        pay_status(self.total, self.paid)
    }
}

pub fn pay_status(total: Money, paid: Money) -> &'static str {
    if total <= 0 || paid >= total {
        "Lunas"
    } else if paid <= 0 {
        "Belum Bayar"
    } else {
        "Dibayar Sebagian"
    }
}

#[derive(Clone, Debug, Default)]
pub struct TxLine {
    pub product_id: String,
    pub name: String,
    pub qty: f64,
    pub price: Money,
    pub cost: Money,
    pub tier: Option<String>,
    pub damaged: f64,
}

#[derive(Clone, Debug, Default)]
pub struct TxDetail {
    pub id: String,
    pub kind: Option<TxKind>,
    pub number: String,
    pub ts: i64,
    pub contact_id: Option<String>,
    pub contact: String,
    pub staff: String,
    pub subtotal: Money,
    pub discount: Money,
    pub fees: Money,
    pub total: Money,
    pub cost_total: Money,
    pub paid: Money,
    pub method: String,
    pub note: String,
    pub ref_id: Option<String>,
    pub lines: Vec<TxLine>,
    pub fee_lines: Vec<(String, Money)>,
    pub payments: Vec<(String, i64, Money, String)>,
    /// Jumlah yang sudah diretur per barang (untuk batas retur).
    pub returned: Vec<(String, f64)>,
    pub refunded: Money,
}

#[derive(Clone, Debug, Default)]
pub struct Expense {
    pub id: String,
    pub ts: i64,
    pub category_id: Option<String>,
    pub category: String,
    pub amount: Money,
    pub note: String,
    pub include_profit: bool,
    pub attachment: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Staff {
    pub id: String,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub active: bool,
}

/// Peran staff yang ada di aplikasi asli. Hak akses default per peran adalah perkiraan.
pub const ROLES: [&str; 5] = ["Pemilik", "Manajer", "Kasir", "Asisten", "Penjaga stok"];

/// Fitur yang bisa diatur aksesnya per staff ("Atur fitur yang dapat diakses staff").
pub const PERMISSIONS: [(&str, &str); 12] = [
    ("pos", "Kasir"),
    ("stock_in", "Stok Masuk"),
    ("stock_out", "Stok Keluar"),
    ("audit", "Stok Audit"),
    ("products", "Atur Barang"),
    ("view_cost", "Lihat Harga Modal"),
    ("history_edit", "Ubah/Hapus Riwayat"),
    ("contacts", "Pembeli & Vendor"),
    ("expenses", "Pengeluaran"),
    ("reports", "Laporan"),
    ("staff", "Atur Staff"),
    ("settings", "Pengaturan"),
];

pub fn default_permissions(role: &str) -> Vec<String> {
    let keys: &[&str] = match role {
        "Pemilik" | "Manajer" => &[
            "pos", "stock_in", "stock_out", "audit", "products", "view_cost", "history_edit", "contacts",
            "expenses", "reports", "staff", "settings",
        ],
        "Kasir" => &["pos", "stock_out", "contacts"],
        "Penjaga stok" => &["stock_in", "stock_out", "audit", "products"],
        _ => &["pos", "stock_in", "stock_out", "contacts"],
    };
    keys.iter().map(|s| s.to_string()).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Dashboard {
    pub sales_today: Money,
    pub profit_today: Money,
    pub tx_today: i64,
    pub low_stock: i64,
    pub near_low: i64,
    pub empty_stock: i64,
    pub expiring: i64,
    pub receivable: Money,
    pub products: i64,
    pub stock_value: Money,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub sales: Money,
    pub discount: Money,
    pub fee_income: Money,
    pub returns: Money,
    pub hpp: Money,
    pub damaged_loss: Money,
    pub gross_profit: Money,
    pub expenses: Money,
    pub net_profit: Money,
    pub purchases: Money,
    pub tx_count: i64,
    pub qty_in: f64,
    pub qty_out: f64,
    pub receivable: Money,
    pub stock_value_cost: Money,
    pub stock_value_sell: Money,
    pub best_sellers: Vec<(String, f64, Money)>,
    pub top_buyers: Vec<(String, Money)>,
    pub daily: Vec<(String, Money)>,
}

// ---------------------------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------------------------

pub struct Db {
    pub conn: Connection,
    pub dir: PathBuf,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA temp_store=MEMORY;
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS categories(id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
  updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS products(id TEXT PRIMARY KEY, code TEXT NOT NULL, name TEXT NOT NULL,
  category_id TEXT, unit TEXT NOT NULL DEFAULT 'pcs', cost_price INTEGER NOT NULL DEFAULT 0,
  sell_price INTEGER NOT NULL DEFAULT 0, stock REAL NOT NULL DEFAULT 0, min_stock REAL NOT NULL DEFAULT 0,
  expiry_date TEXT, location TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
  photo TEXT, thumb TEXT, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE UNIQUE INDEX IF NOT EXISTS idx_products_code ON products(code) WHERE deleted=0;
CREATE INDEX IF NOT EXISTS idx_products_name ON products(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_products_cat ON products(category_id);
CREATE TABLE IF NOT EXISTS price_tiers(id TEXT PRIMARY KEY, product_id TEXT NOT NULL, name TEXT NOT NULL,
  min_qty REAL NOT NULL DEFAULT 1, price INTEGER NOT NULL, updated_at INTEGER NOT NULL DEFAULT 0,
  deleted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_tiers_product ON price_tiers(product_id);
CREATE TABLE IF NOT EXISTS contacts(id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
  phone TEXT NOT NULL DEFAULT '', email TEXT NOT NULL DEFAULT '', address TEXT NOT NULL DEFAULT '',
  category_id TEXT, note TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_contacts_name ON contacts(kind, name COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS payment_methods(id TEXT PRIMARY KEY, name TEXT NOT NULL,
  is_default INTEGER NOT NULL DEFAULT 0, sort INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS fee_templates(id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL,
  value REAL NOT NULL, include_profit INTEGER NOT NULL DEFAULT 1, updated_at INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS transactions(id TEXT PRIMARY KEY, kind TEXT NOT NULL, number TEXT NOT NULL,
  ts INTEGER NOT NULL, contact_id TEXT, staff_id TEXT, subtotal INTEGER NOT NULL DEFAULT 0,
  discount INTEGER NOT NULL DEFAULT 0, fees INTEGER NOT NULL DEFAULT 0, fees_profit INTEGER NOT NULL DEFAULT 0,
  total INTEGER NOT NULL DEFAULT 0, cost_total INTEGER NOT NULL DEFAULT 0, paid INTEGER NOT NULL DEFAULT 0,
  payment_method_id TEXT, ref_id TEXT, note TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_tx_ts ON transactions(ts);
CREATE INDEX IF NOT EXISTS idx_tx_contact ON transactions(contact_id);
CREATE INDEX IF NOT EXISTS idx_tx_ref ON transactions(ref_id);
CREATE TABLE IF NOT EXISTS transaction_items(id TEXT PRIMARY KEY, tx_id TEXT NOT NULL, product_id TEXT NOT NULL,
  name TEXT NOT NULL, qty REAL NOT NULL, price INTEGER NOT NULL, cost INTEGER NOT NULL, tier TEXT,
  damaged REAL NOT NULL DEFAULT 0, stock_after REAL);
CREATE INDEX IF NOT EXISTS idx_items_tx ON transaction_items(tx_id);
CREATE INDEX IF NOT EXISTS idx_items_product ON transaction_items(product_id);
CREATE TABLE IF NOT EXISTS tx_fees(id TEXT PRIMARY KEY, tx_id TEXT NOT NULL, name TEXT NOT NULL,
  kind TEXT NOT NULL, value REAL NOT NULL, amount INTEGER NOT NULL, include_profit INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS payments(id TEXT PRIMARY KEY, tx_id TEXT NOT NULL, ts INTEGER NOT NULL,
  amount INTEGER NOT NULL, method_id TEXT, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_payments_tx ON payments(tx_id);
CREATE TABLE IF NOT EXISTS expenses(id TEXT PRIMARY KEY, ts INTEGER NOT NULL, category_id TEXT,
  amount INTEGER NOT NULL, note TEXT NOT NULL DEFAULT '', include_profit INTEGER NOT NULL DEFAULT 1,
  attachment TEXT, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_expenses_ts ON expenses(ts);
CREATE TABLE IF NOT EXISTS staff(id TEXT PRIMARY KEY, name TEXT NOT NULL, phone TEXT NOT NULL DEFAULT '',
  email TEXT NOT NULL DEFAULT '', role TEXT NOT NULL, permissions TEXT NOT NULL DEFAULT '',
  active INTEGER NOT NULL DEFAULT 1, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS outbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, entity TEXT NOT NULL,
  entity_id TEXT NOT NULL, op TEXT NOT NULL, payload TEXT NOT NULL, ts INTEGER NOT NULL);
"#;

/// Tabel yang ikut sinkron, dan nama entitasnya di API.
pub const SYNC_TABLES: [(&str, &str); 10] = [
    ("categories", "category"),
    ("products", "product"),
    ("price_tiers", "price_tier"),
    ("contacts", "contact"),
    ("payment_methods", "payment_method"),
    ("fee_templates", "fee_template"),
    ("transactions", "transaction"),
    ("payments", "payment"),
    ("expenses", "expense"),
    ("staff", "staff"),
];

pub fn table_for_entity(entity: &str) -> Option<&'static str> {
    SYNC_TABLES.iter().find(|(_, e)| *e == entity).map(|(t, _)| *t)
}

fn entity_for_table(table: &str) -> &'static str {
    SYNC_TABLES.iter().find(|(t, _)| *t == table).map(|(_, e)| *e).unwrap_or("unknown")
}

type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

fn collect<T>(r: rusqlite::Result<rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>>) -> Vec<T> {
    match r {
        Ok(it) => it.filter_map(|x| x.ok()).collect(),
        Err(_) => vec![],
    }
}

impl Db {
    pub fn open(dir: &Path) -> R<Db> {
        std::fs::create_dir_all(dir).map_err(e)?;
        std::fs::create_dir_all(dir.join("photos")).map_err(e)?;
        let conn = Connection::open(dir.join("bakadai_lite.db")).map_err(e)?;
        conn.execute_batch(SCHEMA).map_err(e)?;
        let db = Db { conn, dir: dir.to_path_buf() };
        db.ensure_defaults()?;
        Ok(db)
    }

    fn ensure_defaults(&self) -> R<()> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM payment_methods WHERE deleted=0", [], |r| r.get(0))
            .map_err(e)?;
        if n == 0 {
            self.conn
                .execute(
                    "INSERT INTO payment_methods(id,name,is_default,sort,updated_at) VALUES(?1,'Tunai',1,0,?2)",
                    params![new_id(), now_ms()],
                )
                .map_err(e)?;
            for (i, name) in ["QRIS", "Transfer Bank"].iter().enumerate() {
                self.conn
                    .execute(
                        "INSERT INTO payment_methods(id,name,is_default,sort,updated_at) VALUES(?1,?2,0,?3,?4)",
                        params![new_id(), name, i as i64 + 1, now_ms()],
                    )
                    .map_err(e)?;
            }
        }
        let owners: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM staff WHERE deleted=0", [], |r| r.get(0)).map_err(e)?;
        if owners == 0 {
            let s = Staff {
                name: "Pemilik".into(),
                role: "Pemilik".into(),
                permissions: default_permissions("Pemilik"),
                active: true,
                ..Default::default()
            };
            self.save_staff(&s)?;
        }
        Ok(())
    }

    // ---- pengaturan ----------------------------------------------------------------------

    pub fn setting(&self, key: &str, default: &str) -> String {
        self.conn
            .query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or_else(|| default.to_string())
    }
    pub fn setting_bool(&self, key: &str, default: bool) -> bool {
        self.setting(key, if default { "1" } else { "0" }) == "1"
    }
    pub fn set_setting(&self, key: &str, value: &str) -> R<()> {
        self.conn
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(e)
    }

    // ---- outbox (antrian sinkron) --------------------------------------------------------

    pub fn row_json(&self, table: &str, id: &str) -> R<Value> {
        let sql = format!("SELECT * FROM {} WHERE id=?1", table);
        let mut st = self.conn.prepare(&sql).map_err(e)?;
        let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        st.query_row(params![id], |r| {
            let mut m = Map::new();
            for (i, n) in names.iter().enumerate() {
                let val: rusqlite::types::Value = r.get(i)?;
                m.insert(n.clone(), sql_to_json(val));
            }
            Ok(Value::Object(m))
        })
        .map_err(e)
    }

    /// Untuk transaksi, payload juga memuat item, biaya tambahan dan pembayaran.
    fn payload(&self, table: &str, id: &str) -> Value {
        let mut v = self.row_json(table, id).unwrap_or(Value::Null);
        if table == "transactions" {
            if let Value::Object(m) = &mut v {
                for (key, child) in [("items", "transaction_items"), ("fee_lines", "tx_fees"), ("payments", "payments")] {
                    let ids: Vec<String> = match self.conn.prepare(&format!("SELECT id FROM {} WHERE tx_id=?1", child)) {
                        Ok(mut st) => collect(st.query_map(params![id], |r| r.get::<_, String>(0))),
                        Err(_) => vec![],
                    };
                    let arr: Vec<Value> = ids.iter().filter_map(|cid| self.row_json(child, cid).ok()).collect();
                    m.insert(key.to_string(), Value::Array(arr));
                }
            }
        }
        v
    }

    fn log(&self, table: &str, id: &str, op: &str) -> R<()> {
        let payload = self.payload(table, id);
        self.conn
            .execute(
                "INSERT INTO outbox(entity,entity_id,op,payload,ts) VALUES(?1,?2,?3,?4,?5)",
                params![entity_for_table(table), id, op, payload.to_string(), now_ms()],
            )
            .map(|_| ())
            .map_err(e)
    }

    /// Antrian unggah foto (dikirim terpisah sebagai JPEG yang sudah dikompres).
    pub fn queue_photo_upload(&self, product_id: &str, path: &str) -> R<()> {
        let payload = serde_json::json!({ "product_id": product_id, "path": path });
        self.conn
            .execute(
                "INSERT INTO outbox(entity,entity_id,op,payload,ts) VALUES('photo',?1,'upload',?2,?3)",
                params![product_id, payload.to_string(), now_ms()],
            )
            .map(|_| ())
            .map_err(e)
    }

    pub fn outbox_count(&self) -> i64 {
        self.conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn outbox_batch(&self, limit: i64) -> R<Vec<(i64, String, String, String, Value, i64)>> {
        let mut st = self
            .conn
            .prepare("SELECT seq,entity,entity_id,op,payload,ts FROM outbox ORDER BY seq LIMIT ?1")
            .map_err(e)?;
        let rows = st
            .query_map(params![limit], |r| {
                let p: String = r.get(4)?;
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, serde_json::from_str(&p).unwrap_or(Value::Null), r.get(5)?))
            })
            .map_err(e)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e)
    }

    pub fn outbox_ack(&self, upto_seq: i64) -> R<()> {
        self.conn.execute("DELETE FROM outbox WHERE seq<=?1", params![upto_seq]).map(|_| ()).map_err(e)
    }

    /// Terapkan perubahan dari server (last-write-wins berdasarkan updated_at).
    pub fn apply_remote(&self, entity: &str, data: &Value) -> R<bool> {
        let Some(table) = table_for_entity(entity) else { return Ok(false) };
        let Some(obj) = data.as_object() else { return Ok(false) };
        let Some(id) = obj.get("id").and_then(|v| v.as_str()) else { return Ok(false) };
        let remote_ts = obj.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0);
        let local_ts: Option<i64> = self
            .conn
            .query_row(&format!("SELECT updated_at FROM {} WHERE id=?1", table), params![id], |r| r.get(0))
            .optional()
            .map_err(e)?;
        if let Some(l) = local_ts {
            if l > remote_ts {
                return Ok(false);
            }
        }
        self.upsert_json(table, obj)?;
        if table == "transactions" {
            for (key, child) in [("items", "transaction_items"), ("fee_lines", "tx_fees"), ("payments", "payments")] {
                if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
                    for it in arr {
                        if let Some(o) = it.as_object() {
                            self.upsert_json(child, o)?;
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    fn upsert_json(&self, table: &str, obj: &Map<String, Value>) -> R<()> {
        let cols = self.columns(table)?;
        let keys: Vec<&String> = obj.keys().filter(|k| cols.contains(k)).collect();
        if keys.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "INSERT OR REPLACE INTO {}({}) VALUES({})",
            table,
            keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(","),
            (1..=keys.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",")
        );
        let vals: Vec<rusqlite::types::Value> = keys.iter().map(|k| json_to_sql(&obj[*k])).collect();
        self.conn.execute(&sql, rusqlite::params_from_iter(vals)).map(|_| ()).map_err(e)
    }

    fn columns(&self, table: &str) -> R<Vec<String>> {
        let mut st = self.conn.prepare(&format!("PRAGMA table_info({})", table)).map_err(e)?;
        let rows = st.query_map([], |r| r.get::<_, String>(1)).map_err(e)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e)
    }

    // ---- kategori ------------------------------------------------------------------------

    pub fn categories(&self, kind: &str) -> Vec<Category> {
        let Ok(mut st) = self
            .conn
            .prepare("SELECT id,kind,name FROM categories WHERE kind=?1 AND deleted=0 ORDER BY name COLLATE NOCASE")
        else {
            return vec![];
        };
        collect(st.query_map(params![kind], |r| Ok(Category { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)? })))
    }

    pub fn save_category(&self, id: Option<&str>, kind: &str, name: &str) -> R<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Nama kategori tidak boleh kosong".into());
        }
        let id = id.map(|s| s.to_string()).unwrap_or_else(new_id);
        self.conn
            .execute(
                "INSERT INTO categories(id,kind,name,updated_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, updated_at=excluded.updated_at",
                params![id, kind, name, now_ms()],
            )
            .map_err(e)?;
        self.log("categories", &id, "upsert")?;
        Ok(id)
    }

    pub fn delete_category(&self, id: &str) -> R<()> {
        self.conn
            .execute("UPDATE categories SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        // Barang di kategori ini menjadi "Tanpa Kategori".
        self.conn
            .execute("UPDATE products SET category_id=NULL, updated_at=?2 WHERE category_id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("categories", id, "delete")
    }

    // ---- barang --------------------------------------------------------------------------

    pub fn list_products(&self, search: &str, category: Option<&str>, filter: StockFilter, sort: ProductSort) -> Vec<ProductRow> {
        let mut sql = String::from(
            "SELECT id,code,name,category_id,unit,stock,min_stock,sell_price,cost_price,thumb,expiry_date
             FROM products WHERE deleted=0",
        );
        let mut args: Vec<rusqlite::types::Value> = vec![];
        let s = search.trim();
        if !s.is_empty() {
            sql.push_str(" AND (name LIKE ?1 OR code LIKE ?1)");
            args.push(format!("%{}%", s).into());
        }
        if let Some(c) = category {
            if c == "__none__" {
                sql.push_str(" AND category_id IS NULL");
            } else {
                sql.push_str(&format!(" AND category_id=?{}", args.len() + 1));
                args.push(c.to_string().into());
            }
        }
        match filter {
            StockFilter::All => {}
            StockFilter::Low => sql.push_str(" AND min_stock>0 AND stock<=min_stock AND stock>0"),
            StockFilter::Empty => sql.push_str(" AND stock<=0"),
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(sort.sql());
        let Ok(mut st) = self.conn.prepare(&sql) else { return vec![] };
        collect(st.query_map(rusqlite::params_from_iter(args), |r| {
            Ok(ProductRow {
                id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                category_id: r.get(3)?,
                unit: r.get(4)?,
                stock: r.get(5)?,
                min_stock: r.get(6)?,
                sell_price: r.get(7)?,
                cost_price: r.get(8)?,
                thumb: r.get(9)?,
                expiry_date: r.get(10)?,
            })
        }))
    }

    pub fn product(&self, id: &str) -> Option<Product> {
        let mut p = self
            .conn
            .query_row(
                "SELECT id,code,name,category_id,unit,cost_price,sell_price,stock,min_stock,expiry_date,location,
                 description,photo,thumb FROM products WHERE id=?1",
                params![id],
                |r| {
                    Ok(Product {
                        id: r.get(0)?,
                        code: r.get(1)?,
                        name: r.get(2)?,
                        category_id: r.get(3)?,
                        unit: r.get(4)?,
                        cost_price: r.get(5)?,
                        sell_price: r.get(6)?,
                        stock: r.get(7)?,
                        min_stock: r.get(8)?,
                        expiry_date: r.get(9)?,
                        location: r.get(10)?,
                        description: r.get(11)?,
                        photo: r.get(12)?,
                        thumb: r.get(13)?,
                        tiers: vec![],
                    })
                },
            )
            .optional()
            .ok()
            .flatten()?;
        p.tiers = self.tiers(id);
        Some(p)
    }

    pub fn product_by_code(&self, code: &str) -> Option<Product> {
        let code = normalize_scanned_code(code);
        let id: Option<String> = self
            .conn
            .query_row("SELECT id FROM products WHERE code=?1 AND deleted=0", params![code], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        id.and_then(|i| self.product(&i))
    }

    pub fn tiers(&self, product_id: &str) -> Vec<PriceTier> {
        let Ok(mut st) = self
            .conn
            .prepare("SELECT id,name,min_qty,price FROM price_tiers WHERE product_id=?1 AND deleted=0 ORDER BY min_qty")
        else {
            return vec![];
        };
        collect(st.query_map(params![product_id], |r| {
            Ok(PriceTier { id: r.get(0)?, name: r.get(1)?, min_qty: r.get(2)?, price: r.get(3)? })
        }))
    }

    pub fn code_taken(&self, code: &str, except_id: &str) -> bool {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM products WHERE code=?1 AND id<>?2 AND deleted=0",
                params![code.trim(), except_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0
    }

    /// Tombol "acak" kode barang: 12 digit, dicek tidak bentrok.
    pub fn random_code(&self) -> String {
        loop {
            let id = new_id();
            let digits: String = id.bytes().rev().take(10).map(|b| char::from(b'0' + (b % 10))).collect();
            let code = format!("20{}", digits);
            if !self.code_taken(&code, "") {
                return code;
            }
        }
    }

    /// Simpan barang. Untuk barang baru, `initial_stock` dicatat sebagai Stok Masuk awal.
    pub fn save_product(&mut self, p: &Product, initial_stock: Option<f64>) -> R<String> {
        if p.name.trim().is_empty() {
            return Err("Mohon isi nama barang".into());
        }
        let code = p.code.trim();
        if code.is_empty() {
            return Err("Mohon tambahkan kode barang".into());
        }
        if code.chars().count() > MAX_CODE_LEN {
            return Err("Kode Barang Maksimal 25 Karakter".into());
        }
        if digits(p.sell_price) > MAX_PRICE_DIGITS {
            return Err("Harga Jual Maksimal 13 Digit".into());
        }
        if digits(p.cost_price) > MAX_PRICE_DIGITS {
            return Err("Harga Modal Maksimal 13 Digit".into());
        }
        if p.sell_price < 0 || p.cost_price < 0 || p.min_stock < 0.0 {
            return Err("Tidak boleh angka negatif".into());
        }
        let is_new = p.id.is_empty();
        let id = if is_new { new_id() } else { p.id.clone() };
        if self.code_taken(code, &id) {
            return Err("Kode barang sudah terpakai".into());
        }
        let tx = self.conn.transaction().map_err(e)?;
        tx.execute(
            "INSERT INTO products(id,code,name,category_id,unit,cost_price,sell_price,stock,min_stock,expiry_date,
               location,description,photo,thumb,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(id) DO UPDATE SET code=excluded.code,name=excluded.name,category_id=excluded.category_id,
               unit=excluded.unit,cost_price=excluded.cost_price,sell_price=excluded.sell_price,
               min_stock=excluded.min_stock,expiry_date=excluded.expiry_date,location=excluded.location,
               description=excluded.description,photo=excluded.photo,thumb=excluded.thumb,updated_at=excluded.updated_at",
            params![
                id,
                code,
                p.name.trim(),
                p.category_id,
                if p.unit.trim().is_empty() { "pcs" } else { p.unit.trim() },
                p.cost_price,
                p.sell_price,
                p.min_stock,
                p.expiry_date.as_ref().filter(|s| !s.trim().is_empty()),
                p.location,
                p.description,
                p.photo,
                p.thumb,
                now_ms()
            ],
        )
        .map_err(e)?;
        tx.execute("UPDATE price_tiers SET deleted=1, updated_at=?2 WHERE product_id=?1", params![id, now_ms()])
            .map_err(e)?;
        for t in &p.tiers {
            if t.name.trim().is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR REPLACE INTO price_tiers(id,product_id,name,min_qty,price,updated_at,deleted)
                 VALUES(?1,?2,?3,?4,?5,?6,0)",
                params![
                    if t.id.is_empty() { new_id() } else { t.id.clone() },
                    id,
                    t.name.trim(),
                    t.min_qty.max(1.0),
                    t.price,
                    now_ms()
                ],
            )
            .map_err(e)?;
        }
        tx.commit().map_err(e)?;
        self.log("products", &id, "upsert")?;
        for t in self.tiers(&id) {
            self.log("price_tiers", &t.id, "upsert")?;
        }
        if is_new {
            if let Some(q) = initial_stock.filter(|q| *q > 0.0) {
                let lines = vec![CartLine {
                    product_id: id.clone(),
                    name: p.name.trim().to_string(),
                    qty: q,
                    price: p.cost_price,
                    ..Default::default()
                }];
                let total = compute_totals(&lines, false, 0.0, &[]).total;
                // Stok awal dianggap sudah lunas (bukan hutang ke vendor).
                self.create_tx(&NewTx {
                    kind: Some(TxKind::In),
                    ts: now_ms(),
                    lines,
                    note: STOCK_INITIAL_NOTE.into(),
                    paid: total,
                    ..Default::default()
                })?;
            }
        }
        Ok(id)
    }

    pub fn delete_products(&self, ids: &[String]) -> R<()> {
        for id in ids {
            self.conn
                .execute("UPDATE products SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
                .map_err(e)?;
            self.log("products", id, "delete")?;
        }
        Ok(())
    }

    pub fn set_photo(&self, id: &str, photo: &str, thumb: &str) -> R<()> {
        self.conn
            .execute("UPDATE products SET photo=?2, thumb=?3, updated_at=?4 WHERE id=?1", params![id, photo, thumb, now_ms()])
            .map_err(e)?;
        self.log("products", id, "upsert")
    }

    pub fn product_count(&self) -> i64 {
        self.conn.query_row("SELECT COUNT(*) FROM products WHERE deleted=0", [], |r| r.get(0)).unwrap_or(0)
    }

    /// Riwayat per barang: (jenis, waktu, nomor, jumlah, stok sesudah), terbaru dulu.
    pub fn product_history(&self, product_id: &str, limit: i64) -> Vec<(TxKind, i64, String, f64, Option<f64>)> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT t.kind, t.ts, t.number, i.qty, i.stock_after FROM transaction_items i
             JOIN transactions t ON t.id=i.tx_id WHERE i.product_id=?1 AND t.deleted=0 ORDER BY t.ts DESC LIMIT ?2",
        ) else {
            return vec![];
        };
        collect(st.query_map(params![product_id, limit], |r| {
            Ok((TxKind::from_code(&r.get::<_, String>(0)?), r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        }))
    }

    // ---- transaksi stok ------------------------------------------------------------------

    fn next_number(&self, kind: TxKind, ts: i64) -> String {
        let day = chrono::DateTime::from_timestamp_millis(ts)
            .map(|d| d.with_timezone(&chrono::Local).format("%y%m%d").to_string())
            .unwrap_or_default();
        let (start, end) = day_bounds(ts);
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE kind=?1 AND ts>=?2 AND ts<?3",
                params![kind.code(), start, end],
                |r| r.get(0),
            )
            .unwrap_or(0);
        format!("{}-{}-{:04}", kind.prefix(), day, n + 1)
    }

    pub fn lock_days(&self) -> i64 {
        self.setting("lock_days", "0").parse().unwrap_or(0)
    }

    fn check_lock(&self, ts: i64) -> R<()> {
        let days = self.lock_days();
        if days > 0 && ts < now_ms() - days * 86_400_000 {
            return Err(format!(
                "Transaksi stok yang berusia lebih dari {} hari tidak dapat ditambahkan mundur, diubah, atau dihapus.",
                days
            ));
        }
        Ok(())
    }

    /// Buat transaksi stok (masuk/keluar/audit/retur) dan perbarui stok + harga modal secara atomik.
    pub fn create_tx(&mut self, t: &NewTx) -> R<String> {
        let kind = t.kind.ok_or("Jenis transaksi kosong")?;
        if t.lines.is_empty() {
            return Err("Pilih minimal satu barang".into());
        }
        self.check_lock(t.ts)?;
        let allow_negative = self.setting_bool("allow_negative", false);
        let auto_avg = self.setting_bool("auto_avg_cost", true);
        let id = new_id();
        let number = self.next_number(kind, t.ts);
        let totals = match kind {
            TxKind::Audit => Totals::default(),
            TxKind::SaleReturn | TxKind::PurchaseReturn => Totals { subtotal: t.refund, total: t.refund, ..Default::default() },
            _ => compute_totals(&t.lines, t.discount_pct, t.discount_value, &t.fees),
        };
        // Uang lebih dari total = kembalian; yang tercatat dibayar maksimal sebesar total.
        let paid = match kind {
            TxKind::Audit => 0,
            TxKind::SaleReturn | TxKind::PurchaseReturn => t.refund,
            _ => t.paid.min(totals.total).max(0),
        };
        let tx = self.conn.transaction().map_err(e)?;
        let mut cost_total: Money = 0;
        let mut touched = vec![];
        for l in &t.lines {
            if l.qty <= 0.0 && kind != TxKind::Audit {
                return Err("Jumlah harus lebih besar dari 0".into());
            }
            let (stock, cost): (f64, Money) = tx
                .query_row("SELECT stock,cost_price FROM products WHERE id=?1", params![l.product_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map_err(|_| format!("Barang tidak ditemukan: {}", l.name))?;
            let (new_stock, new_cost, item_qty, item_cost) = match kind {
                TxKind::In => {
                    let nc = if auto_avg { weighted_cost(stock, cost, l.qty, l.price) } else { cost };
                    (stock + l.qty, nc, l.qty, l.price)
                }
                TxKind::Out => {
                    if !allow_negative && stock < l.qty {
                        return Err(format!("Stok tidak mencukupi: {} (sisa {})", l.name, fmt_qty(stock)));
                    }
                    cost_total += (cost as f64 * l.qty).round() as Money;
                    (stock - l.qty, cost, l.qty, cost)
                }
                TxKind::Audit => {
                    if l.qty < 0.0 && !allow_negative {
                        return Err("Tidak boleh angka negatif".into());
                    }
                    let diff = l.qty - stock;
                    if diff.abs() < 1e-9 {
                        return Err("Selisih Audit Tidak Boleh Nol".into());
                    }
                    (l.qty, cost, diff, cost)
                }
                TxKind::SaleReturn => {
                    if l.damaged > l.qty {
                        return Err("Jumlah rusak tidak boleh melebihi jumlah retur".into());
                    }
                    let good = (l.qty - l.damaged).max(0.0);
                    cost_total += (cost as f64 * good).round() as Money;
                    (stock + good, cost, l.qty, cost)
                }
                TxKind::PurchaseReturn => {
                    if !allow_negative && stock < l.qty {
                        return Err(format!("Stok tidak mencukupi: {}", l.name));
                    }
                    (stock - l.qty, cost, l.qty, cost)
                }
            };
            tx.execute(
                "UPDATE products SET stock=?2, cost_price=?3, updated_at=?4 WHERE id=?1",
                params![l.product_id, new_stock, new_cost, now_ms()],
            )
            .map_err(e)?;
            tx.execute(
                "INSERT INTO transaction_items(id,tx_id,product_id,name,qty,price,cost,tier,damaged,stock_after)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![new_id(), id, l.product_id, l.name, item_qty, l.price, item_cost, l.tier, l.damaged, new_stock],
            )
            .map_err(e)?;
            touched.push(l.product_id.clone());
        }
        if kind == TxKind::In {
            cost_total = totals.subtotal;
        }
        let base = totals.subtotal - totals.discount;
        for f in &t.fees {
            tx.execute(
                "INSERT INTO tx_fees(id,tx_id,name,kind,value,amount,include_profit) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![new_id(), id, f.name, f.kind, f.value, fee_amount(f, base), f.include_profit as i64],
            )
            .map_err(e)?;
        }
        tx.execute(
            "INSERT INTO transactions(id,kind,number,ts,contact_id,staff_id,subtotal,discount,fees,fees_profit,total,
               cost_total,paid,payment_method_id,ref_id,note,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                id,
                kind.code(),
                number,
                t.ts,
                t.contact_id,
                t.staff_id,
                totals.subtotal,
                totals.discount,
                totals.fees,
                totals.fees_profit,
                totals.total,
                cost_total,
                paid,
                t.payment_method_id,
                t.ref_id,
                t.note,
                now_ms()
            ],
        )
        .map_err(e)?;
        if paid > 0 {
            tx.execute(
                "INSERT INTO payments(id,tx_id,ts,amount,method_id,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![new_id(), id, t.ts, paid, t.payment_method_id, now_ms()],
            )
            .map_err(e)?;
        }
        tx.commit().map_err(e)?;
        self.log("transactions", &id, "upsert")?;
        for p in touched {
            self.log("products", &p, "upsert")?;
        }
        Ok(id)
    }

    /// Hapus transaksi dan kembalikan efek stoknya.
    pub fn delete_tx(&mut self, id: &str) -> R<()> {
        let d = self.tx_detail(id).ok_or("Transaksi tidak ditemukan")?;
        self.check_lock(d.ts)?;
        let kind = d.kind.unwrap_or(TxKind::Out);
        let tx = self.conn.transaction().map_err(e)?;
        for l in &d.lines {
            let delta = match kind {
                TxKind::In => -l.qty,
                TxKind::Out => l.qty,
                TxKind::Audit => -l.qty,
                TxKind::SaleReturn => -(l.qty - l.damaged),
                TxKind::PurchaseReturn => l.qty,
            };
            tx.execute("UPDATE products SET stock=stock+?2, updated_at=?3 WHERE id=?1", params![l.product_id, delta, now_ms()])
                .map_err(e)?;
        }
        tx.execute("UPDATE transactions SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()]).map_err(e)?;
        tx.execute("UPDATE payments SET deleted=1, updated_at=?2 WHERE tx_id=?1", params![id, now_ms()]).map_err(e)?;
        tx.commit().map_err(e)?;
        self.log("transactions", id, "delete")?;
        for l in &d.lines {
            self.log("products", &l.product_id, "upsert")?;
        }
        Ok(())
    }

    pub fn add_payment(&self, tx_id: &str, amount: Money, method_id: Option<String>, ts: i64) -> R<()> {
        let (total, paid, tx_ts): (Money, Money, i64) = self
            .conn
            .query_row("SELECT total,paid,ts FROM transactions WHERE id=?1", params![tx_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|_| "Transaksi tidak ditemukan".to_string())?;
        if amount <= 0 {
            return Err("Jumlah harus lebih besar dari 0".into());
        }
        if amount > total - paid {
            return Err("Jumlah melebihi sisa piutang".into());
        }
        let ts = ts.max(tx_ts); // "Tanggal pembayaran tidak boleh sebelum tanggal transaksi"
        let pid = new_id();
        self.conn
            .execute(
                "INSERT INTO payments(id,tx_id,ts,amount,method_id,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![pid, tx_id, ts, amount, method_id, now_ms()],
            )
            .map_err(e)?;
        self.conn
            .execute("UPDATE transactions SET paid=paid+?2, updated_at=?3 WHERE id=?1", params![tx_id, amount, now_ms()])
            .map_err(e)?;
        self.log("transactions", tx_id, "upsert")
    }

    pub fn list_tx(&self, kind: Option<TxKind>, from: i64, to: i64, search: &str, unpaid_only: bool, limit: i64) -> Vec<TxRow> {
        let mut sql = String::from(
            "SELECT t.id,t.kind,t.number,t.ts,COALESCE(c.name,''),COALESCE(s.name,''),t.total,t.paid,
               (SELECT COUNT(*) FROM transaction_items i WHERE i.tx_id=t.id),
               (SELECT COALESCE(SUM(ABS(i.qty)),0) FROM transaction_items i WHERE i.tx_id=t.id),
               (SELECT GROUP_CONCAT(n, ', ') FROM (SELECT name AS n FROM transaction_items WHERE tx_id=t.id LIMIT 3))
             FROM transactions t LEFT JOIN contacts c ON c.id=t.contact_id LEFT JOIN staff s ON s.id=t.staff_id
             WHERE t.deleted=0 AND t.ts>=?1 AND t.ts<?2",
        );
        let mut args: Vec<rusqlite::types::Value> = vec![from.into(), to.into()];
        if let Some(k) = kind {
            sql.push_str(" AND t.kind=?3");
            args.push(k.code().to_string().into());
        }
        if unpaid_only {
            sql.push_str(" AND t.paid < t.total AND t.kind IN ('out','in')");
        }
        let s = search.trim();
        if !s.is_empty() {
            sql.push_str(&format!(
                " AND (t.number LIKE ?{n} OR c.name LIKE ?{n} OR EXISTS(SELECT 1 FROM transaction_items i WHERE i.tx_id=t.id AND i.name LIKE ?{n}))",
                n = args.len() + 1
            ));
            args.push(format!("%{}%", s).into());
        }
        sql.push_str(&format!(" ORDER BY t.ts DESC LIMIT {}", limit.max(1)));
        let Ok(mut st) = self.conn.prepare(&sql) else { return vec![] };
        collect(st.query_map(rusqlite::params_from_iter(args), |r| {
            Ok(TxRow {
                id: r.get(0)?,
                kind: TxKind::from_code(&r.get::<_, String>(1)?),
                number: r.get(2)?,
                ts: r.get(3)?,
                contact: r.get(4)?,
                staff: r.get(5)?,
                total: r.get(6)?,
                paid: r.get(7)?,
                items: r.get(8)?,
                qty: r.get(9)?,
                summary: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
            })
        }))
    }

    pub fn tx_detail(&self, id: &str) -> Option<TxDetail> {
        let mut d = self
            .conn
            .query_row(
                "SELECT t.id,t.kind,t.number,t.ts,t.contact_id,COALESCE(c.name,''),COALESCE(s.name,''),t.subtotal,
                   t.discount,t.fees,t.total,t.cost_total,t.paid,COALESCE(m.name,''),t.note,t.ref_id
                 FROM transactions t LEFT JOIN contacts c ON c.id=t.contact_id LEFT JOIN staff s ON s.id=t.staff_id
                 LEFT JOIN payment_methods m ON m.id=t.payment_method_id WHERE t.id=?1",
                params![id],
                |r| {
                    Ok(TxDetail {
                        id: r.get(0)?,
                        kind: Some(TxKind::from_code(&r.get::<_, String>(1)?)),
                        number: r.get(2)?,
                        ts: r.get(3)?,
                        contact_id: r.get(4)?,
                        contact: r.get(5)?,
                        staff: r.get(6)?,
                        subtotal: r.get(7)?,
                        discount: r.get(8)?,
                        fees: r.get(9)?,
                        total: r.get(10)?,
                        cost_total: r.get(11)?,
                        paid: r.get(12)?,
                        method: r.get(13)?,
                        note: r.get(14)?,
                        ref_id: r.get(15)?,
                        ..Default::default()
                    })
                },
            )
            .optional()
            .ok()
            .flatten()?;
        if let Ok(mut st) =
            self.conn.prepare("SELECT product_id,name,qty,price,cost,tier,damaged FROM transaction_items WHERE tx_id=?1")
        {
            d.lines = collect(st.query_map(params![id], |r| {
                Ok(TxLine {
                    product_id: r.get(0)?,
                    name: r.get(1)?,
                    qty: r.get(2)?,
                    price: r.get(3)?,
                    cost: r.get(4)?,
                    tier: r.get(5)?,
                    damaged: r.get(6)?,
                })
            }));
        }
        if let Ok(mut st) = self.conn.prepare("SELECT name,amount FROM tx_fees WHERE tx_id=?1") {
            d.fee_lines = collect(st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?))));
        }
        if let Ok(mut st) = self.conn.prepare(
            "SELECT p.id,p.ts,p.amount,COALESCE(m.name,'') FROM payments p LEFT JOIN payment_methods m ON m.id=p.method_id
             WHERE p.tx_id=?1 AND p.deleted=0 ORDER BY p.ts",
        ) {
            d.payments = collect(st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))));
        }
        if let Ok(mut st) = self.conn.prepare(
            "SELECT i.product_id, SUM(i.qty) FROM transaction_items i JOIN transactions t ON t.id=i.tx_id
             WHERE t.ref_id=?1 AND t.deleted=0 GROUP BY i.product_id",
        ) {
            d.returned = collect(st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?))));
        }
        d.refunded = self
            .conn
            .query_row("SELECT COALESCE(SUM(total),0) FROM transactions WHERE ref_id=?1 AND deleted=0", params![id], |r| r.get(0))
            .unwrap_or(0);
        Some(d)
    }

    // ---- kontak --------------------------------------------------------------------------

    pub fn contacts(&self, kind: &str, search: &str) -> Vec<Contact> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT id,kind,name,phone,email,address,category_id,note FROM contacts
             WHERE kind=?1 AND deleted=0 AND (name LIKE ?2 OR phone LIKE ?2) ORDER BY name COLLATE NOCASE",
        ) else {
            return vec![];
        };
        collect(st.query_map(params![kind, format!("%{}%", search.trim())], |r| {
            Ok(Contact {
                id: r.get(0)?,
                kind: r.get(1)?,
                name: r.get(2)?,
                phone: r.get(3)?,
                email: r.get(4)?,
                address: r.get(5)?,
                category_id: r.get(6)?,
                note: r.get(7)?,
            })
        }))
    }

    pub fn save_contact(&self, c: &Contact) -> R<String> {
        if c.name.trim().is_empty() {
            return Err("Nama harus diisi".into());
        }
        let id = if c.id.is_empty() { new_id() } else { c.id.clone() };
        self.conn
            .execute(
                "INSERT INTO contacts(id,kind,name,phone,email,address,category_id,note,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,phone=excluded.phone,email=excluded.email,
                   address=excluded.address,category_id=excluded.category_id,note=excluded.note,updated_at=excluded.updated_at",
                params![id, c.kind, c.name.trim(), c.phone, c.email, c.address, c.category_id, c.note, now_ms()],
            )
            .map_err(e)?;
        self.log("contacts", &id, "upsert")?;
        Ok(id)
    }

    pub fn delete_contact(&self, id: &str) -> R<()> {
        self.conn
            .execute("UPDATE contacts SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("contacts", id, "delete")
    }

    /// Piutang per pembeli (penjualan yang belum lunas): (id, nama, sisa, jumlah transaksi).
    pub fn receivables(&self) -> Vec<(String, String, Money, i64)> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT COALESCE(c.id,''), COALESCE(c.name,'Tanpa nama'), SUM(t.total-t.paid), COUNT(*)
             FROM transactions t LEFT JOIN contacts c ON c.id=t.contact_id
             WHERE t.deleted=0 AND t.kind='out' AND t.paid<t.total GROUP BY c.id ORDER BY 3 DESC",
        ) else {
            return vec![];
        };
        collect(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))))
    }

    // ---- metode pembayaran & biaya tambahan ----------------------------------------------

    pub fn payment_methods(&self) -> Vec<PaymentMethod> {
        let Ok(mut st) =
            self.conn.prepare("SELECT id,name,is_default FROM payment_methods WHERE deleted=0 ORDER BY sort, name")
        else {
            return vec![];
        };
        collect(st.query_map([], |r| Ok(PaymentMethod { id: r.get(0)?, name: r.get(1)?, is_default: r.get::<_, i64>(2)? == 1 })))
    }

    pub fn save_payment_method(&self, id: Option<&str>, name: &str) -> R<()> {
        if name.trim().is_empty() {
            return Err("Nama Metode Pembayaran (Wajib)".into());
        }
        let id = id.map(String::from).unwrap_or_else(new_id);
        let sort: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(sort),0)+1 FROM payment_methods", [], |r| r.get(0)).unwrap_or(9);
        self.conn
            .execute(
                "INSERT INTO payment_methods(id,name,sort,updated_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, updated_at=excluded.updated_at",
                params![id, name.trim(), sort, now_ms()],
            )
            .map_err(e)?;
        self.log("payment_methods", &id, "upsert")
    }

    pub fn delete_payment_method(&self, id: &str) -> R<()> {
        let is_default: i64 = self
            .conn
            .query_row("SELECT is_default FROM payment_methods WHERE id=?1", params![id], |r| r.get(0))
            .unwrap_or(0);
        if is_default == 1 {
            return Err("Metode pembayaran default tidak dapat diubah".into());
        }
        self.conn
            .execute("UPDATE payment_methods SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("payment_methods", id, "delete")
    }

    pub fn fee_templates(&self) -> Vec<FeeTemplate> {
        let Ok(mut st) =
            self.conn.prepare("SELECT id,name,kind,value,include_profit FROM fee_templates WHERE deleted=0 ORDER BY name")
        else {
            return vec![];
        };
        collect(st.query_map([], |r| {
            Ok(FeeTemplate {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                value: r.get(3)?,
                include_profit: r.get::<_, i64>(4)? == 1,
            })
        }))
    }

    pub fn save_fee_template(&self, f: &FeeTemplate) -> R<()> {
        if f.name.trim().is_empty() {
            return Err("Nama harus diisi".into());
        }
        if f.kind == "percent" && f.value > MAX_SURCHARGE_PCT {
            return Err("Maksimum surcharge adalah 99.99%".into());
        }
        if f.value <= 0.0 {
            return Err("Tidak boleh kosong atau nol".into());
        }
        let id = if f.id.is_empty() { new_id() } else { f.id.clone() };
        self.conn
            .execute(
                "INSERT INTO fee_templates(id,name,kind,value,include_profit,updated_at) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,kind=excluded.kind,value=excluded.value,
                   include_profit=excluded.include_profit,updated_at=excluded.updated_at",
                params![id, f.name.trim(), f.kind, f.value, f.include_profit as i64, now_ms()],
            )
            .map_err(e)?;
        self.log("fee_templates", &id, "upsert")
    }

    pub fn delete_fee_template(&self, id: &str) -> R<()> {
        self.conn
            .execute("UPDATE fee_templates SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("fee_templates", id, "delete")
    }

    // ---- pengeluaran ---------------------------------------------------------------------

    pub fn expenses(&self, from: i64, to: i64) -> Vec<Expense> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT x.id,x.ts,x.category_id,COALESCE(c.name,'Tanpa Kategori'),x.amount,x.note,x.include_profit,x.attachment
             FROM expenses x LEFT JOIN categories c ON c.id=x.category_id
             WHERE x.deleted=0 AND x.ts>=?1 AND x.ts<?2 ORDER BY x.ts DESC",
        ) else {
            return vec![];
        };
        collect(st.query_map(params![from, to], |r| {
            Ok(Expense {
                id: r.get(0)?,
                ts: r.get(1)?,
                category_id: r.get(2)?,
                category: r.get(3)?,
                amount: r.get(4)?,
                note: r.get(5)?,
                include_profit: r.get::<_, i64>(6)? == 1,
                attachment: r.get(7)?,
            })
        }))
    }

    pub fn save_expense(&self, x: &Expense) -> R<()> {
        if x.amount <= 0 {
            return Err("Tidak boleh kosong atau nol".into());
        }
        let id = if x.id.is_empty() { new_id() } else { x.id.clone() };
        self.conn
            .execute(
                "INSERT INTO expenses(id,ts,category_id,amount,note,include_profit,attachment,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(id) DO UPDATE SET ts=excluded.ts,category_id=excluded.category_id,amount=excluded.amount,
                   note=excluded.note,include_profit=excluded.include_profit,attachment=excluded.attachment,
                   updated_at=excluded.updated_at",
                params![id, x.ts, x.category_id, x.amount, x.note, x.include_profit as i64, x.attachment, now_ms()],
            )
            .map_err(e)?;
        self.log("expenses", &id, "upsert")
    }

    pub fn delete_expense(&self, id: &str) -> R<()> {
        self.conn
            .execute("UPDATE expenses SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("expenses", id, "delete")
    }

    // ---- staff (tanpa batas jumlah) -------------------------------------------------------

    pub fn staff(&self) -> Vec<Staff> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT id,name,phone,email,role,permissions,active FROM staff WHERE deleted=0
             ORDER BY CASE role WHEN 'Pemilik' THEN 0 ELSE 1 END, name COLLATE NOCASE",
        ) else {
            return vec![];
        };
        collect(st.query_map([], |r| {
            let perms: String = r.get(5)?;
            Ok(Staff {
                id: r.get(0)?,
                name: r.get(1)?,
                phone: r.get(2)?,
                email: r.get(3)?,
                role: r.get(4)?,
                permissions: perms.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
                active: r.get::<_, i64>(6)? == 1,
            })
        }))
    }

    pub fn save_staff(&self, s: &Staff) -> R<String> {
        if s.name.trim().is_empty() {
            return Err("Nama harus diisi".into());
        }
        let id = if s.id.is_empty() { new_id() } else { s.id.clone() };
        self.conn
            .execute(
                "INSERT INTO staff(id,name,phone,email,role,permissions,active,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,phone=excluded.phone,email=excluded.email,
                   role=excluded.role,permissions=excluded.permissions,active=excluded.active,updated_at=excluded.updated_at",
                params![id, s.name.trim(), s.phone, s.email, s.role, s.permissions.join(","), s.active as i64, now_ms()],
            )
            .map_err(e)?;
        self.log("staff", &id, "upsert")?;
        Ok(id)
    }

    pub fn delete_staff(&self, id: &str) -> R<()> {
        let role: String =
            self.conn.query_row("SELECT role FROM staff WHERE id=?1", params![id], |r| r.get(0)).unwrap_or_default();
        if role == "Pemilik" {
            return Err("Pemilik tidak bisa dihapus".into());
        }
        self.conn
            .execute("UPDATE staff SET deleted=1, updated_at=?2 WHERE id=?1", params![id, now_ms()])
            .map_err(e)?;
        self.log("staff", id, "delete")
    }

    // ---- laporan -------------------------------------------------------------------------

    pub fn dashboard(&self) -> Dashboard {
        let (start, end) = day_bounds(now_ms());
        let r = self.report(start, end);
        let q = |sql: &str| -> i64 { self.conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
        let soon = (chrono::Local::now() + chrono::Duration::days(EXPIRY_WARN_DAYS)).format("%Y-%m-%d").to_string();
        let expiring: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM products WHERE deleted=0 AND expiry_date IS NOT NULL AND expiry_date<>'' AND expiry_date<=?1",
                params![soon],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Dashboard {
            sales_today: r.sales - r.returns,
            profit_today: r.net_profit,
            tx_today: r.tx_count,
            low_stock: q("SELECT COUNT(*) FROM products WHERE deleted=0 AND min_stock>0 AND stock>0 AND stock<=min_stock"),
            near_low: q(&format!(
                "SELECT COUNT(*) FROM products WHERE deleted=0 AND min_stock>0 AND stock>min_stock AND stock<=min_stock*{}",
                NEAR_LOW_FACTOR
            )),
            empty_stock: q("SELECT COUNT(*) FROM products WHERE deleted=0 AND stock<=0"),
            expiring,
            receivable: q("SELECT COALESCE(SUM(total-paid),0) FROM transactions WHERE deleted=0 AND kind='out' AND paid<total"),
            products: q("SELECT COUNT(*) FROM products WHERE deleted=0"),
            stock_value: q("SELECT COALESCE(CAST(SUM(MAX(stock,0)*cost_price) AS INTEGER),0) FROM products WHERE deleted=0"),
        }
    }

    pub fn report(&self, from: i64, to: i64) -> Report {
        let mut r = Report::default();
        let sum = |sql: &str| -> Money { self.conn.query_row(sql, params![from, to], |x| x.get(0)).unwrap_or(0) };
        let sales_row: (Money, Money, Money, Money, i64) = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(subtotal),0),COALESCE(SUM(discount),0),COALESCE(SUM(fees_profit),0),
                   COALESCE(SUM(cost_total),0),COUNT(*)
                 FROM transactions WHERE deleted=0 AND kind='out' AND ts>=?1 AND ts<?2",
                params![from, to],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?, x.get(4)?)),
            )
            .unwrap_or_default();
        r.sales = sales_row.0 - sales_row.1;
        r.discount = sales_row.1;
        r.fee_income = sales_row.2;
        r.hpp = sales_row.3;
        r.tx_count = sales_row.4;
        r.returns = sum("SELECT COALESCE(SUM(total),0) FROM transactions WHERE deleted=0 AND kind='sale_return' AND ts>=?1 AND ts<?2");
        // Retur kondisi baik mengembalikan harga modal (HPP berkurang); barang rusak tetap jadi kerugian.
        r.hpp -= sum("SELECT COALESCE(SUM(cost_total),0) FROM transactions WHERE deleted=0 AND kind='sale_return' AND ts>=?1 AND ts<?2");
        r.damaged_loss = sum(
            "SELECT COALESCE(CAST(SUM(i.damaged*i.cost) AS INTEGER),0) FROM transaction_items i JOIN transactions t ON t.id=i.tx_id
             WHERE t.deleted=0 AND t.kind='sale_return' AND t.ts>=?1 AND t.ts<?2",
        );
        r.gross_profit = r.sales - r.returns + r.fee_income - r.hpp;
        r.expenses = sum("SELECT COALESCE(SUM(amount),0) FROM expenses WHERE deleted=0 AND include_profit=1 AND ts>=?1 AND ts<?2");
        r.net_profit = r.gross_profit - r.expenses;
        r.purchases = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total),0) FROM transactions WHERE deleted=0 AND kind='in' AND note<>?3 AND ts>=?1 AND ts<?2",
                params![from, to, STOCK_INITIAL_NOTE],
                |x| x.get(0),
            )
            .unwrap_or(0);
        let qty = |kind: &str| -> f64 {
            self.conn
                .query_row(
                    "SELECT COALESCE(SUM(i.qty),0) FROM transaction_items i JOIN transactions t ON t.id=i.tx_id
                     WHERE t.deleted=0 AND t.kind=?3 AND t.ts>=?1 AND t.ts<?2",
                    params![from, to, kind],
                    |x| x.get(0),
                )
                .unwrap_or(0.0)
        };
        r.qty_in = qty("in");
        r.qty_out = qty("out");
        r.receivable = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total-paid),0) FROM transactions WHERE deleted=0 AND kind='out' AND paid<total",
                [],
                |x| x.get(0),
            )
            .unwrap_or(0);
        let (cv, sv): (Money, Money) = self
            .conn
            .query_row(
                "SELECT COALESCE(CAST(SUM(MAX(stock,0)*cost_price) AS INTEGER),0), COALESCE(CAST(SUM(MAX(stock,0)*sell_price) AS INTEGER),0)
                 FROM products WHERE deleted=0",
                [],
                |x| Ok((x.get(0)?, x.get(1)?)),
            )
            .unwrap_or_default();
        r.stock_value_cost = cv;
        r.stock_value_sell = sv;
        if let Ok(mut st) = self.conn.prepare(
            "SELECT i.name, SUM(i.qty), CAST(SUM(i.qty*i.price) AS INTEGER) FROM transaction_items i JOIN transactions t ON t.id=i.tx_id
             WHERE t.deleted=0 AND t.kind='out' AND t.ts>=?1 AND t.ts<?2 GROUP BY i.product_id ORDER BY 2 DESC LIMIT 10",
        ) {
            r.best_sellers = collect(st.query_map(params![from, to], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?))));
        }
        if let Ok(mut st) = self.conn.prepare(
            "SELECT c.name, SUM(t.total) FROM transactions t JOIN contacts c ON c.id=t.contact_id
             WHERE t.deleted=0 AND t.kind='out' AND t.ts>=?1 AND t.ts<?2 GROUP BY c.id ORDER BY 2 DESC LIMIT 5",
        ) {
            r.top_buyers = collect(st.query_map(params![from, to], |x| Ok((x.get(0)?, x.get(1)?))));
        }
        // Penjualan harian: min 7, maks 31 hari terakhir sampai hari ini (hari kosong = 0), satu query per hari.
        let to = to.min(day_bounds(now_ms()).1);
        let days = ((to - from) / 86_400_000).clamp(7, 31);
        let start = to - days * 86_400_000;
        let mut buckets = vec![0 as Money; days as usize];
        if let Ok(mut st) = self.conn.prepare(
            "SELECT (ts-?1)/86400000, SUM(subtotal-discount) FROM transactions
             WHERE deleted=0 AND kind='out' AND ts>=?1 AND ts<?2 GROUP BY 1",
        ) {
            for (i, v) in collect(st.query_map(params![start, to], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, Money>(1)?)))) {
                if i >= 0 && (i as usize) < buckets.len() {
                    buckets[i as usize] = v;
                }
            }
        }
        for (i, v) in buckets.into_iter().enumerate() {
            let d = start + i as i64 * 86_400_000;
            let label = chrono::DateTime::from_timestamp_millis(d)
                .map(|x| x.with_timezone(&chrono::Local).format("%d/%m").to_string())
                .unwrap_or_default();
            r.daily.push((label, v));
        }
        r
    }

    // ---- ekspor CSV (lebih ringan dari Excel, bisa dibuka di Excel/Sheets) -----------------

    pub fn export_csv(&self, what: &str, from: i64, to: i64) -> R<PathBuf> {
        let dir = self.dir.join("exports");
        std::fs::create_dir_all(&dir).map_err(e)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("{}-{}.csv", what, stamp));
        let mut out = String::from("\u{feff}");
        let cur = self.setting("currency", "Rp");
        match what {
            "barang" => {
                out.push_str("Kode;Nama;Kategori;Satuan;Stok;Stok Minimal;Harga Modal;Harga Jual;Kadaluwarsa\n");
                let cats: std::collections::HashMap<String, String> =
                    self.categories("product").into_iter().map(|c| (c.id, c.name)).collect();
                for p in self.list_products("", None, StockFilter::All, ProductSort::NameAsc) {
                    out.push_str(&format!(
                        "{};{};{};{};{};{};{};{};{}\n",
                        csv(&p.code),
                        csv(&p.name),
                        csv(p.category_id.as_ref().and_then(|c| cats.get(c)).map(|s| s.as_str()).unwrap_or("")),
                        csv(&p.unit),
                        fmt_qty(p.stock),
                        fmt_qty(p.min_stock),
                        money_to_f64(p.cost_price),
                        money_to_f64(p.sell_price),
                        p.expiry_date.unwrap_or_default(),
                    ));
                }
            }
            "riwayat" => {
                out.push_str("Tanggal;Nomor;Jenis;Kontak;Staff;Barang;Jumlah;Total;Dibayar;Status\n");
                for t in self.list_tx(None, from, to, "", false, 1_000_000_000) {
                    out.push_str(&format!(
                        "{};{};{};{};{};{};{};{};{};{}\n",
                        fmt_dt(t.ts),
                        t.number,
                        t.kind.label(),
                        csv(&t.contact),
                        csv(&t.staff),
                        csv(&t.summary),
                        fmt_qty(t.qty),
                        money_to_f64(t.total),
                        money_to_f64(t.paid),
                        t.pay_status()
                    ));
                }
            }
            "pengeluaran" => {
                out.push_str("Tanggal;Kategori;Nominal;Catatan;Termasuk Profit\n");
                for x in self.expenses(from, to) {
                    out.push_str(&format!(
                        "{};{};{};{};{}\n",
                        fmt_dt(x.ts),
                        csv(&x.category),
                        money_to_f64(x.amount),
                        csv(&x.note),
                        if x.include_profit { "Ya" } else { "Tidak" }
                    ));
                }
            }
            _ => {
                let r = self.report(from, to);
                out.push_str("Laporan;Nilai\n");
                for (k, v) in [
                    ("Penjualan", r.sales),
                    ("Retur", r.returns),
                    ("Biaya tambahan (profit)", r.fee_income),
                    ("Harga modal terjual (HPP)", r.hpp),
                    ("Laba kotor", r.gross_profit),
                    ("Pengeluaran", r.expenses),
                    ("Laba bersih", r.net_profit),
                    ("Pembelian", r.purchases),
                    ("Piutang", r.receivable),
                ] {
                    out.push_str(&format!("{};{}\n", k, fmt_money(v, &cur)));
                }
            }
        }
        std::fs::write(&path, out).map_err(e)?;
        Ok(path)
    }

    // ---- data contoh ---------------------------------------------------------------------

    pub fn seed_demo(&mut self) -> R<()> {
        if self.product_count() > 0 {
            return Ok(());
        }
        let food = self.save_category(None, "product", "Makanan & Minuman")?;
        let home = self.save_category(None, "product", "Kebutuhan Rumah")?;
        let stat = self.save_category(None, "product", "Alat Kantor & Buku")?;
        self.save_category(None, "expense", "Operasional")?;
        self.save_category(None, "expense", "Gaji")?;
        let samples: [(&str, &str, &str, f64, f64, f64, f64); 10] = [
            ("Kopi Bubuk 200g", "8990001", food.as_str(), 18000.0, 25000.0, 40.0, 10.0),
            ("Teh Celup isi 25", "8990002", food.as_str(), 6500.0, 9000.0, 8.0, 10.0),
            ("Gula Pasir 1kg", "8990003", food.as_str(), 14000.0, 17500.0, 60.0, 15.0),
            ("Mie Instan Goreng", "8990004", food.as_str(), 2600.0, 3500.0, 120.0, 40.0),
            ("Air Mineral 600ml", "8990005", food.as_str(), 2200.0, 4000.0, 14.0, 12.0),
            ("Sabun Cuci Piring", "8990006", home.as_str(), 9500.0, 13000.0, 25.0, 5.0),
            ("Tisu Gulung", "8990007", home.as_str(), 3000.0, 5000.0, 0.0, 6.0),
            ("Deterjen 800g", "8990008", home.as_str(), 17000.0, 22000.0, 18.0, 5.0),
            ("Buku Tulis 38 lbr", "8990009", stat.as_str(), 3200.0, 5000.0, 75.0, 20.0),
            ("Pulpen Hitam", "8990010", stat.as_str(), 1800.0, 3000.0, 4.0, 10.0),
        ];
        let mut ids = vec![];
        for (name, code, cat, cost, sell, stock, min) in samples {
            let p = Product {
                code: code.into(),
                name: name.into(),
                category_id: Some(cat.to_string()),
                unit: "pcs".into(),
                cost_price: money_from_f64(cost),
                sell_price: money_from_f64(sell),
                min_stock: min,
                tiers: if code == "8990004" {
                    vec![PriceTier { name: "Grosir (1 dus)".into(), min_qty: 40.0, price: money_from_f64(3100.0), ..Default::default() }]
                } else {
                    vec![]
                },
                expiry_date: if code == "8990005" {
                    Some((chrono::Local::now() + chrono::Duration::days(12)).format("%Y-%m-%d").to_string())
                } else {
                    None
                },
                ..Default::default()
            };
            let initial = if stock > 0.0 { Some(stock) } else { None };
            let id = self.save_product(&p, initial)?;
            ids.push((id, name, money_from_f64(sell)));
        }
        let buyer = self.save_contact(&Contact {
            kind: "buyer".into(),
            name: "Bu Rina (Warung)".into(),
            phone: "0812000111".into(),
            ..Default::default()
        })?;
        self.save_contact(&Contact {
            kind: "vendor".into(),
            name: "Toko Grosir Sentosa".into(),
            phone: "0813000222".into(),
            ..Default::default()
        })?;
        let cash = self.payment_methods().into_iter().find(|m| m.is_default).map(|m| m.id);
        let (today, _) = day_bounds(now_ms());
        let base = (now_ms() - 60_000).max(today + 60_000);
        let picks = [(0usize, 2.0, 1u8), (3, 5.0, 0), (2, 1.0, 1), (8, 3.0, 0), (5, 1.0, 1), (7, 2.0, 2), (4, 4.0, 1)];
        for (n, (idx, qty, who)) in picks.iter().enumerate() {
            let (id, name, price) = &ids[*idx];
            let lines = vec![CartLine { product_id: id.clone(), name: name.to_string(), qty: *qty, price: *price, ..Default::default() }];
            let tot = compute_totals(&lines, false, 0.0, &[]).total;
            self.create_tx(&NewTx {
                kind: Some(TxKind::Out),
                ts: (base - (n as i64) * 600_000).max(today + 1000),
                contact_id: if *who == 2 { Some(buyer.clone()) } else { None },
                lines,
                paid: if *who == 2 { 0 } else { tot },
                payment_method_id: cash.clone(),
                ..Default::default()
            })?;
        }
        self.conn.execute("DELETE FROM outbox", []).map_err(e)?;
        Ok(())
    }
}

pub const STOCK_INITIAL_NOTE: &str = "Stok awal";

/// Kode hasil scan: semua format barcode diterima (seperti aplikasi asli, `Fv0->t` memakai opsi bawaan
/// "semua format" bila tidak ada daftar format). Scanner USB/Bluetooth sering menambah spasi, Tab, CR/LF
/// atau karakter tak terlihat — itu dibuang supaya kode cocok dengan data barang.
pub fn normalize_scanned_code(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_control() && *c != '\u{feff}' && *c != '\u{200b}').collect::<String>().trim().to_string()
}

fn digits(m: Money) -> usize {
    (m.abs() / 100).to_string().len()
}

fn csv(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn fmt_dt(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

pub fn fmt_date(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y").to_string())
        .unwrap_or_default()
}

/// Awal & akhir hari lokal yang memuat `ts`.
pub fn day_bounds(ts: i64) -> (i64, i64) {
    use chrono::TimeZone;
    let d = chrono::DateTime::from_timestamp_millis(ts).unwrap_or_default().with_timezone(&chrono::Local);
    let start = chrono::Local
        .from_local_datetime(&d.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default())
        .earliest()
        .map(|x| x.timestamp_millis())
        .unwrap_or(ts);
    (start, start + 86_400_000)
}

fn sql_to_json(v: rusqlite::types::Value) -> Value {
    use rusqlite::types::Value as V;
    match v {
        V::Null => Value::Null,
        V::Integer(i) => Value::from(i),
        V::Real(f) => Value::from(f),
        V::Text(s) => Value::from(s),
        V::Blob(_) => Value::Null,
    }
}

fn json_to_sql(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as V;
    match v {
        Value::Null => V::Null,
        Value::Bool(b) => V::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => V::Integer(i),
            None => V::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => V::Text(s.clone()),
        other => V::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_average() {
        // 10 @ 1000 + 10 @ 2000 → 1500
        assert_eq!(weighted_cost(10.0, 100_000, 10.0, 200_000), 150_000);
        assert_eq!(weighted_cost(0.0, 100_000, 5.0, 300_000), 300_000);
    }

    #[test]
    fn totals_with_discount_and_fee() {
        let lines = vec![CartLine { qty: 2.0, price: 1_000_000, ..Default::default() }];
        let fees = vec![FeeLine { name: "Layanan".into(), kind: "percent".into(), value: 10.0, include_profit: true }];
        let t = compute_totals(&lines, true, 10.0, &fees);
        assert_eq!(t.subtotal, 2_000_000);
        assert_eq!(t.discount, 200_000);
        assert_eq!(t.fees, 180_000);
        assert_eq!(t.total, 1_980_000);
        assert_eq!(compute_totals(&lines, true, 150.0, &[]).total, 0);
    }

    #[test]
    fn money_format() {
        assert_eq!(fmt_money(1_250_000, "Rp"), "Rp 12.500");
        assert_eq!(fmt_money(12_345_678_900, "Rp"), "Rp 123.456.789");
    }


    #[test]
    fn scanned_code_cleanup() {
        assert_eq!(normalize_scanned_code(" 8990001\r\n"), "8990001");
        assert_eq!(normalize_scanned_code("\u{feff}8990001\t"), "8990001");
        assert_eq!(normalize_scanned_code("ABC-123"), "ABC-123");
    }

    #[test]
    fn tier_pick() {
        let tiers = vec![
            PriceTier { name: "A".into(), min_qty: 10.0, price: 900, ..Default::default() },
            PriceTier { name: "B".into(), min_qty: 50.0, price: 800, ..Default::default() },
        ];
        assert_eq!(tier_price(1000, &tiers, 5.0).0, 1000);
        assert_eq!(tier_price(1000, &tiers, 12.0).0, 900);
        assert_eq!(tier_price(1000, &tiers, 60.0).0, 800);
    }

    #[test]
    fn stock_flow_end_to_end() {
        let dir = std::env::temp_dir().join(format!("kl-test-{}", new_id()));
        let mut db = Db::open(&dir).unwrap();
        db.seed_demo().unwrap();
        let p = db.product_by_code("8990001").unwrap();
        // 40 awal − 2 terjual = 38
        assert!((p.stock - 38.0).abs() < 1e-9);
        // Stok masuk 2 @ 30.000 → rata-rata (38×18.000 + 2×30.000)/40 = 18.600
        db.create_tx(&NewTx {
            kind: Some(TxKind::In),
            ts: now_ms(),
            lines: vec![CartLine { product_id: p.id.clone(), name: p.name.clone(), qty: 2.0, price: 3_000_000, ..Default::default() }],
            ..Default::default()
        })
        .unwrap();
        let p2 = db.product(&p.id).unwrap();
        assert_eq!(p2.cost_price, 1_860_000);
        assert!(db.outbox_count() > 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
