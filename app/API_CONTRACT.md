# Kontrak REST API — Bakadai Lite

Dokumen ini untuk Anda yang akan membuat server. Aplikasi sudah memanggil endpoint bertanda **[dipakai app]**;
sisanya disarankan untuk panel web/admin dan fitur berikutnya. Semua endpoint di bawah awalan `/v1`.

## 1. Aturan umum

| Hal | Aturan |
|---|---|
| Format | JSON UTF-8 (`Content-Type: application/json`), kecuali unggah foto (`image/jpeg`). |
| Login | Header `Authorization: Bearer <token>`. Token terikat ke **satu bisnis** (lihat 3. Bisnis). |
| ID | String dibuat **di HP** (22 karakter heksadesimal, urut waktu), mis. `0194f3a2b1c0a8e3d9f1c2`. Server **wajib menerima ID dari klien** (supaya bisa offline). |
| Uang | **Bilangan bulat dalam 1/100** (sen). `Rp 12.500` dikirim `1250000`. Tidak ada angka desimal untuk uang. |
| Jumlah barang | Angka desimal (boleh 1.5 untuk kg). |
| Waktu | Milidetik sejak 1970 UTC (`ts`, `updated_at`). Tanggal kadaluwarsa: teks `YYYY-MM-DD`. |
| Hapus | **Soft delete**: kolom `deleted` = 1, baris tetap ada supaya bisa disinkron. |
| Konflik | **Last-write-wins** berdasarkan `updated_at` (yang lebih baru menang). |
| Idempotensi | Header `Idempotency-Key` di POST/PUT. Kunci sama = jangan diproses dua kali, kembalikan respons pertama. |
| Daftar | `?limit=` (default 50, maks 500) dan `?cursor=` → respons `{ "data": [...], "next_cursor": "..." | null }`. |
| Error | Status HTTP yang sesuai + `{ "error": { "code": "STOCK_NOT_ENOUGH", "message": "Stok tidak mencukupi", "field": "items[0].qty" } }` |

Kode error yang disarankan (pesan sama dengan teks di aplikasi):

| code | HTTP | message |
|---|---|---|
| `VALIDATION` | 422 | sesuai kolom (mis. "Mohon isi nama barang") |
| `CODE_TAKEN` | 409 | Kode barang sudah terpakai |
| `STOCK_NOT_ENOUGH` | 409 | Stok tidak mencukupi |
| `AUDIT_ZERO_DIFF` | 422 | Selisih Audit Tidak Boleh Nol |
| `TX_LOCKED` | 409 | Transaksi stok yang berusia lebih dari N hari tidak dapat ditambahkan mundur, diubah, atau dihapus. |
| `PAYMENT_EXCEEDS` | 422 | Jumlah melebihi sisa piutang |
| `RETURN_EXCEEDS` | 422 | Jumlah retur tidak boleh melebihi transaksi awal |
| `REFUND_EXCEEDS` | 422 | Nominal refund tidak boleh melebihi batas refund transaksi awal |
| `NO_ACCESS` | 403 | Anda tidak memiliki akses fitur ini. |
| `UNAUTHORIZED` | 401 | Sesi berakhir, silakan masuk ulang |
| `OUTSIDE_WORK_HOURS` | 403 | Di luar jam kerja Anda |

**Tidak ada batas jumlah** staff, barang, riwayat, atau biaya tambahan. Jangan buat aturan kuota di server.

## 2. Autentikasi

### `GET /v1/health` [dipakai app]
Tanpa login. `200 { "ok": true, "version": "1.0.0", "time": 1794890851444 }`

### `POST /v1/auth/login` [dipakai app]
```json
{ "email": "pemilik@toko.com", "password": "rahasia", "device_id": "dev-0194f3a2b1c0a8e3d9f1c2" }
```
`200`
```json
{
  "token": "eyJ...",
  "refresh_token": "r1...",
  "expires_at": 1795495651444,
  "user": { "id": "u_01", "name": "Andi", "email": "pemilik@toko.com" },
  "business": { "id": "b_01", "name": "Toko Saya", "currency": "Rp" },
  "staff": { "id": "0194...", "role": "Pemilik", "permissions": ["pos","stock_in","..."] }
}
```
Aplikasi menyimpan `token`. Disarankan juga: `POST /v1/auth/refresh` `{refresh_token}` → token baru,
`POST /v1/auth/logout`, `POST /v1/auth/magic-link` `{email}` (aplikasi asli punya login lewat tautan email),
`POST /v1/auth/google` `{id_token}`.

### `GET /v1/me`
Profil + daftar bisnis yang bisa diakses: `{ "user": {...}, "businesses": [{ "id", "name", "role" }] }`

### `POST /v1/auth/switch-business` `{ "business_id": "b_02" }` → token baru untuk bisnis itu.

## 3. Bisnis & undangan staff
- `GET /v1/businesses`, `POST /v1/businesses` `{ "name", "category", "currency", "address", "phone" }`,
  `PATCH /v1/businesses/{id}`, `DELETE /v1/businesses/{id}` (konfirmasi dengan `{ "confirm_name": "Toko Saya" }`).
- `POST /v1/businesses/{id}/invites` → `{ "code": "K7Q2MX", "expires_at": ... }` — kode berlaku **48 jam**.
- `POST /v1/invites/join` `{ "code": "K7Q2MX" }` → `{ "business": {...}, "staff": {...} }`.
  Error: "Kode undangan tidak valid, atau anda sudah bergabung ke bisnis tersebut."

## 4. Sinkron (inti offline-first)

### `POST /v1/sync/push` [dipakai app]
Header `Idempotency-Key: <device_id>-<seq terakhir>`. Aplikasi mengirim maks 200 perubahan per panggilan, berurutan.
```json
{
  "device_id": "dev-0194f3a2b1c0a8e3d9f1c2",
  "changes": [
    { "seq": 41, "entity": "product", "id": "0194f3...", "op": "upsert",
      "data": { "id": "0194f3...", "code": "8990001", "name": "Kopi Bubuk 200g", "category_id": "0194aa...",
                "unit": "pcs", "cost_price": 1800000, "sell_price": 2500000, "stock": 38, "min_stock": 10,
                "expiry_date": null, "location": "", "description": "", "photo": "/data/.../p01.jpg",
                "thumb": "/data/.../p01_t.jpg", "updated_at": 1794890851444, "deleted": 0 },
      "client_ts": 1794890851444 },
    { "seq": 42, "entity": "transaction", "id": "0194f4...", "op": "upsert",
      "data": { "id": "0194f4...", "kind": "out", "number": "OUT-261117-0003", "ts": 1794890900000,
                "contact_id": null, "staff_id": "0194...", "subtotal": 4250000, "discount": 0, "fees": 0,
                "fees_profit": 0, "total": 4250000, "cost_total": 3120000, "paid": 4250000,
                "payment_method_id": "0194...", "ref_id": null, "note": "", "updated_at": 1794890900000, "deleted": 0,
                "items": [ { "id": "...", "tx_id": "0194f4...", "product_id": "0194f3...", "name": "Kopi Bubuk 200g",
                             "qty": 1, "price": 2500000, "cost": 1800000, "tier": null, "damaged": 0, "stock_after": 37 } ],
                "fee_lines": [], "payments": [ { "id": "...", "tx_id": "0194f4...", "ts": 1794890900000, "amount": 4250000,
                             "method_id": "0194...", "updated_at": 1794890900000, "deleted": 0 } ] },
      "client_ts": 1794890900000 },
    { "seq": 43, "entity": "staff", "id": "0194...", "op": "delete", "data": { "id": "0194...", "deleted": 1, "updated_at": ... } }
  ]
}
```
`entity` ∈ `category, product, price_tier, contact, payment_method, fee_template, transaction, payment, expense, staff`.
`op` ∈ `upsert, delete`. Respons `200 { "accepted": [41,42,43], "rejected": [ { "seq": 44, "error": {...} } ] }`.
Aplikasi menganggap batch terkirim jika status 2xx. Perubahan yang ditolak sebaiknya dikembalikan lewat `pull`
dengan versi server (supaya HP ikut versi yang benar).

**Penting soal stok:** transaksi adalah sumber kebenaran. Saat menerima transaksi baru, server menerapkan efek stoknya
(lihat bagian 7) lalu menaikkan `updated_at` produk; jangan menimpa stok server dengan angka `stock` dari HP kecuali
produk itu baru. Untuk produk yang sudah ada, ambil kolom selain `stock` dari HP.

### `PUT /v1/products/{id}/photo` [dipakai app]
Body: byte JPEG yang **sudah dikompres di HP** (≤ 1024 px, biasanya 60–200 KB). `Content-Type: image/jpeg`.
Respons `200 { "photo_url": "https://cdn.../p01.jpg", "thumb_url": "https://cdn.../p01_t.jpg" }`.
Server boleh membuat thumbnail sendiri. Tolak > 10 MB (`413`).

### `GET /v1/sync/pull?since=<cursor>&limit=500&device_id=...` [dipakai app]
```json
{
  "changes": [ { "entity": "product", "op": "upsert", "data": { ...baris lengkap... } },
               { "entity": "contact", "op": "delete", "data": { "id": "...", "updated_at": ... } } ],
  "cursor": "1794890999999-000123",
  "has_more": false
}
```
`since=0` = semua data. `cursor` bebas formatnya (aplikasi menyimpannya apa adanya), harus naik terus.
Jangan kirim balik perubahan yang berasal dari `device_id` yang sama (opsional, hemat kuota).
Transaksi dikirim lengkap dengan `items`, `fee_lines`, `payments` seperti di push.

## 5. Bentuk data (sama dengan kolom database di HP)

| Entitas | Kolom |
|---|---|
| category | `id, kind ("product"/"expense"/"contact"), name, updated_at, deleted` |
| product | `id, code (≤25, unik per bisnis), name, category_id?, unit, cost_price, sell_price, stock, min_stock, expiry_date?, location, description, photo?, thumb?, updated_at, deleted` |
| price_tier | `id, product_id, name, min_qty, price, updated_at, deleted` |
| contact | `id, kind ("buyer"/"vendor"), name, phone, email, address, category_id?, note, updated_at, deleted` |
| payment_method | `id, name, is_default (0/1), sort, updated_at, deleted` |
| fee_template | `id, name, kind ("percent"/"fixed"), value (persen atau uang dalam satuan utuh), include_profit (0/1), updated_at, deleted` |
| transaction | `id, kind, number, ts, contact_id?, staff_id?, subtotal, discount, fees, fees_profit, total, cost_total, paid, payment_method_id?, ref_id?, note, updated_at, deleted, items[], fee_lines[], payments[]` (`fees` = total biaya, angka; `fee_lines` = rincian) |
| transaction item | `id, tx_id, product_id, name, qty, price, cost, tier?, damaged, stock_after` |
| tx fee | `id, tx_id, name, kind, value, amount, include_profit` |
| payment | `id, tx_id, ts, amount, method_id?, updated_at, deleted` |
| expense | `id, ts, category_id?, amount, note, include_profit (0/1), attachment?, updated_at, deleted` |
| staff | `id, name, phone, email, role, permissions ("pos,stock_in,..." dipisah koma), active (0/1), updated_at, deleted` |

`transaction.kind`: `in` (stok masuk/pembelian), `out` (stok keluar/penjualan, termasuk kasir), `audit`,
`sale_return` (retur penjualan, `ref_id` = transaksi awal), `purchase_return`.
Untuk `audit`, `items[].qty` = selisih (boleh negatif) dan `stock_after` = stok hasil hitung.

Hak akses (`permissions`): `pos, stock_in, stock_out, audit, products, view_cost, history_edit, contacts, expenses,
reports, staff, settings`. Peran: `Pemilik, Manajer, Kasir, Asisten, Penjaga stok`.

## 6. Endpoint REST biasa (untuk panel web / integrasi)

Semua mendukung `GET` daftar (dengan `limit`/`cursor`), `GET /{id}`, `POST` (ID dari klien boleh), `PATCH /{id}`, `DELETE /{id}` (soft delete).

| Sumber | Path | Filter daftar |
|---|---|---|
| Kategori | `/v1/categories` | `kind` |
| Barang | `/v1/products` | `search` (nama/kode), `category_id` (`none` = tanpa kategori), `stock=low|near_low|empty`, `sort=name_asc|name_desc|stock_asc|stock_desc|newest|expiry`, `updated_since` |
| Barang per kode | `GET /v1/products/by-code/{code}` | untuk scanner barcode |
| Harga grosir | `/v1/products/{id}/price-tiers` (`PUT` mengganti semua) | |
| Riwayat barang | `GET /v1/products/{id}/history` | `from`, `to` |
| Stok pada tanggal | `GET /v1/products/{id}/stock-at?date=YYYY-MM-DD` | ("Cek stok pada") |
| Impor massal | `POST /v1/products/import` (CSV/XLSX, kolom sama dengan ekspor) | |
| Pembeli & vendor | `/v1/contacts` | `kind`, `search`, `category_id`, `has_debt=true` |
| Metode bayar | `/v1/payment-methods` | metode default tidak bisa dihapus |
| Biaya tambahan | `/v1/fee-templates` | |
| Transaksi | `/v1/transactions` | `kind`, `from`, `to`, `contact_id`, `staff_id`, `unpaid=true`, `search` |
| Pembayaran | `POST /v1/transactions/{id}/payments` `{ id, ts, amount, method_id }`, `DELETE /v1/payments/{id}` | |
| Retur | `POST /v1/transactions/{id}/returns` `{ id, ts, items:[{product_id, qty, damaged}], refund }` | |
| Pengeluaran | `/v1/expenses` | `from`, `to`, `category_id` |
| Staff | `/v1/staff` | `active` |
| Jam kerja staff | `PUT /v1/staff/{id}/work-hours` `{ timezone, days: { "mon": [["08:00","12:00"],["13:00","17:00"]] } }` | maks 3 periode/hari, min 15 menit, tidak tumpang tindih |
| Transfer stok antar bisnis | `POST /v1/stock-transfers` `{ id, target_business_id, items:[{product_id, qty}], use_average_cost }` | fitur aplikasi asli, belum ada di app |
| Pengaturan bisnis | `GET/PATCH /v1/settings` `{ allow_negative, auto_avg_cost, lock_days, receipt_footer, paper_mm, currency }` | |

### Laporan
- `GET /v1/reports/summary?from=&to=` →
  `{ sales, discount, fee_income, returns, hpp, damaged_loss, gross_profit, expenses, net_profit, purchases, tx_count, qty_in, qty_out, receivable, stock_value_cost, stock_value_sell }`
- `GET /v1/reports/daily-sales?from=&to=` → `[ { "date": "2026-11-17", "sales": 17300000 } ]`
- `GET /v1/reports/best-sellers?from=&to=&limit=10` → `[ { product_id, name, qty, amount } ]`
- `GET /v1/reports/top-buyers?from=&to=` → `[ { contact_id, name, amount } ]`
- `GET /v1/reports/receivables` → `[ { contact_id, name, outstanding, tx_count } ]`
- `GET /v1/reports/consolidated?from=&to=` → ringkasan gabungan semua bisnis milik user.
- `GET /v1/exports/{products|transactions|expenses}.csv?from=&to=` → file CSV (pemisah `;`, BOM UTF-8).

## 7. Aturan bisnis yang harus sama di server

1. **Stok masuk**: `stock += qty`. Bila `auto_avg_cost` aktif: `cost = ((stok_lama × modal_lama) + (qty × modal_masuk)) ÷ (stok_lama + qty)`; stok lama ≤ 0 → `cost = modal_masuk`.
2. **Stok keluar**: tolak bila `stock < qty` dan `allow_negative` mati. `cost_total = Σ qty × harga_modal saat itu`.
3. **Audit**: `stock = hasil_hitung`; selisih 0 ditolak; tidak masuk laporan keuangan.
4. **Retur penjualan**: barang baik kembali ke stok, rusak tidak; `cost_total` = modal barang baik; refund ≤ (max(dibayar,total) − refund sebelumnya); qty ≤ sisa yang belum diretur.
5. **Retur pembelian**: `stock -= qty`.
6. **Hapus transaksi**: balikkan efek stok; tolak bila lebih tua dari `lock_days` (0 = tidak dikunci).
7. **Total**: `subtotal = Σ round(price × qty)`; diskon persen ≤ 100% atau nominal ≤ subtotal; biaya persen (≤ 99,99%) dihitung dari `subtotal − discount`; `total = subtotal − discount + Σ biaya`.
8. **Dibayar**: `paid = min(uang_diterima, total)`; status `Lunas` bila `paid ≥ total`, `Belum Bayar` bila 0, selain itu `Dibayar Sebagian`. Pembayaran tambahan ≤ sisa dan tanggal ≥ tanggal transaksi.
9. **Laba**: kotor = (subtotal − diskon) − refund retur + biaya yang `include_profit` − (HPP − modal retur baik); bersih = kotor − pengeluaran `include_profit`.
10. **Hak akses**: cek `permissions` staff pada setiap endpoint (mis. `POST /transactions kind=in` butuh `stock_in`; melihat `cost_price` butuh `view_cost` — sembunyikan kolom modal bila tidak punya).
11. **Nomor transaksi**: format app `OUT-YYMMDD-0001` per jenis per hari di HP; server boleh membuat nomor sendiri, tapi simpan nomor dari HP agar struk tetap cocok.

## 8. Rekomendasi teknis server
- Database: PostgreSQL, semua tabel punya `business_id`, indeks `(business_id, updated_at)` untuk `pull`, unik `(business_id, code) WHERE deleted=0` untuk barang.
- Foto: simpan di object storage (S3/R2/MinIO) + CDN; HP sudah mengirim JPEG kecil, jadi biaya bandwidth rendah.
- Kompres respons (`Content-Encoding: gzip`) untuk `pull`.
- Rate limit login ("Terlalu banyak percobaan. Tunggu sebentar sebelum mencoba lagi." → 429).
