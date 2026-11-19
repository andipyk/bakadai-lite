# Handover untuk developer — Bakadai Lite

Dokumen ini untuk developer yang melanjutkan proyek. Ringkasan produk & cara pakai ada di [`README.md`](../README.md),
kontrak server di `API_CONTRACT.md`, aturan hitungan di `MECHANICS.md`.

## 1. Teknologi & alasannya

| Bagian | Pilihan | Alasan |
|---|---|---|
| Bahasa | Rust (edition 2021, toolchain stable 1.100) | Native, cepat, APK kecil, satu kode untuk Android & komputer |
| Tampilan | egui / eframe 0.37 (backend `glow` = OpenGL ES) | Immediate-mode, tanpa Java/Kotlin, ringan; versi komputer dipakai untuk pratinjau & cek otomatis |
| Database | SQLite (rusqlite 0.32, `bundled`, mode WAL) | Offline-first, cepat untuk ratusan ribu baris |
| Gambar | `image` 0.25 (jpeg/png/webp) | Kompres foto di HP |
| HTTP | `ureq` 2 (rustls) | Sinkron di thread terpisah, tanpa runtime async |
| Android | `android-native-activity` (NativeActivity) + `cargo-apk` | Tanpa kode Java; APK 3,6 MB |

## 2. Struktur kode

```
app/
├─ Cargo.toml          dependensi, profil rilis (opt-level s, LTO, strip), metadata APK
├─ build-apk.sh        membuat APK (memasang alat ke app/.tools bila belum ada)
├─ src/
│  ├─ main.rs          titik masuk komputer; opsi --screenshot <png> [--screen <nama>]
│  ├─ lib.rs           modul + android_main (titik masuk Android) + folder data komputer
│  ├─ db.rs            SEMUA data & aturan bisnis: skema SQLite, model, transaksi stok,
│  │                   harga modal rata-rata, total/diskon/biaya, retur, laporan, ekspor CSV,
│  │                   outbox sinkron, data contoh, unit test
│  ├─ sync.rs          health, login, push/pull (thread terpisah, hasil lewat channel)
│  ├─ imgproc.rs       kompres foto (rumus ukuran dari aplikasi asli), thumbnail
│  ├─ escpos.rs        struk teks lebar tetap + perintah ESC/POS + cetak TCP port 9100
│  └─ ui/
│     ├─ mod.rs        BakadaiApp: navigasi (stack layar), top bar, menu bawah, toast,
│     │                konfirmasi, cache thumbnail (LRU 96), screenshot
│     ├─ widgets.rs    tema (warna, ukuran sentuh), ikon digambar sendiri, kartu, tombol, baris daftar
│     ├─ products.rs   daftar barang (gulir virtual), detail & riwayat barang, form barang + foto
│     ├─ stock.rs      form Stok Masuk/Keluar/Audit, Riwayat, detail transaksi, struk, retur
│     ├─ pos.rs        Kasir: grid barang, keranjang, diskon, biaya, pembayaran, sukses
│     └─ more.rs       Beranda, menu Lainnya, kontak, piutang, pengeluaran, staff,
│                      laporan, pengaturan, kategori, metode bayar, biaya tambahan, sinkron
```

## 3. Cara menjalankan, menguji, membuat APK

```
cd app
cargo run                                        # jendela seukuran HP (412×860)
cargo test                                       # 10 unit test (aturan stok, total, foto, scanner…)
cargo run -- --screenshot shots/home.png         # simpan gambar layar lalu keluar
cargo run -- --screenshot shots/pos.png --screen pos
#   --screen: home | pos | products | history | more | reports | staff | stockin | settings | edit | sync
sh build-apk.sh                                  # APK rilis bertanda tangan → target/release/apk/BakadaiLite.apk
adb install -r target/release/apk/BakadaiLite.apk # pasang ke HP lewat USB
adb logcat | grep -i bakadai                      # lihat log/crash di HP
```
Mode `--screenshot` memakai database sementara berisi data contoh, jadi data asli tidak tersentuh.
Data versi komputer ada di `~/Library/Application Support/BakadaiLite` (macOS); bisa diganti dengan
variabel `BAKADAI_LITE_DATA=/path`. Di Android: folder internal aplikasi.

Kebutuhan alat build APK (sudah dipasang di `app/.tools`): rustup + target `aarch64-linux-android`,
`cargo-apk` 0.10, Android SDK (`~/Library/Android/sdk`, build-tools 36, platform 36), NDK 28,
Java (keytool/apksigner). Tambah HP 32-bit: `rustup target add armv7-linux-androideabi` lalu tambahkan
ke `build_targets` di `Cargo.toml`.

## 4. Arsitektur & alur data

```
 Layar (egui, tiap frame)  ──baca──►  cache di state layar (dimuat ulang bila app.rev berubah)
        │ aksi pengguna                         ▲
        ▼                                       │ app.changed() → rev+1
   db.rs (Db) ── transaksi SQLite atomik ── tulis tabel + outbox(seq, entity, id, op, payload JSON)
                                                │
                       sync.rs (thread) ◄──── outbox_batch(200) → POST /v1/sync/push → outbox_ack
                                         ────► GET /v1/sync/pull → apply_remote (last-write-wins)
```
- **Pola layar**: setiap layar punya struct state (`ProductsState`, `StockState`, `PosState`, `MoreState`).
  Daftar disimpan di state dan hanya di-query ulang bila `app.rev` atau kunci filter berubah —
  jangan query database di setiap frame untuk daftar besar.
- **Daftar besar** memakai `ScrollArea::show_rows` (hanya baris terlihat yang digambar) → tetap mulus
  walau barang ratusan ribu.
- **Setiap perubahan data** wajib lewat fungsi di `db.rs`, yang memanggil `self.log(table, id, op)`
  supaya masuk antrean sinkron. Lalu panggil `app.changed()` di UI.
- **Uang** selalu `Money = i64` dalam 1/100. Konversi hanya di UI (`parse_money`, `fmt_money`).
- **ID** dibuat dengan `db::new_id()` (urut waktu + acak), aman dibuat offline di banyak HP.
- **Stok** hanya berubah lewat `create_tx` / `delete_tx` (Masuk, Keluar, Audit, Retur). Jangan update
  kolom `stock` langsung.
- **Hak akses**: `app.can("perm")`; daftar izin di `db::PERMISSIONS`, default per peran di
  `db::default_permissions`.

## 5. Resep: menambah sesuatu

**Kolom baru di barang (contoh: `brand`)**
1. `db.rs` → `SCHEMA`: tambah `brand TEXT NOT NULL DEFAULT ''` di `CREATE TABLE products`.
   Untuk database yang sudah ada, tambahkan migrasi di `Db::open` setelah `execute_batch`:
   `let _ = conn.execute("ALTER TABLE products ADD COLUMN brand TEXT NOT NULL DEFAULT ''", []);`
2. Tambah field di `Product` (dan `ProductRow` bila tampil di daftar), di `product()`, `save_product()`.
3. `ui/products.rs` → `ProductForm` + `open_edit` + `edit_screen` + simpan.
4. Sinkron otomatis ikut (payload = seluruh baris). Tambahkan kolomnya di `API_CONTRACT.md` bagian 5.

**Layar baru**
1. Tambah varian di `enum Screen` (`ui/mod.rs`) + judul di `Screen::title`.
2. Tulis `pub fn nama_screen(app: &mut BakadaiApp, ui: &mut Ui)` di file ui yang sesuai.
3. Daftarkan di `match screen` dalam `impl eframe::App` dan buka dengan `app.go(Screen::...)`.

**Endpoint server baru dipanggil dari app**
Tambah fungsi di `sync.rs` mengikuti pola `health`/`login` (`run_in_thread` + `SyncResult`),
tangani hasilnya di `BakadaiApp::poll_sync`.

## 6. Hal yang perlu diketahui (jebakan)

- Font bawaan egui **tidak punya simbol** (⌂ ▦ ✓ → …) → tampil kotak kosong. Pakai `widgets::paint_icon`
  atau teks biasa. Huruf Indonesia, "·", "…" aman.
- `TextEdit::desired_width` sudah termasuk margin; `ComboBox::width` perlu dikurangi ±30 px agar tidak
  melebar keluar kartu.
- Di macOS, jendela yang tertutup jendela lain berhenti menggambar; mode screenshot mengirim
  `ViewportCommand::RequestPaintWhileHidden` agar tetap jalan.
- egui di Android: **keyboard layar** muncul lewat winit (`set_ime_allowed`); di sebagian HP mungkin perlu
  memanggil `showSoftInput` lewat JNI (crate `jni`) — **belum diuji di HP sungguhan**.
- Foto di database disimpan sebagai path lokal; setelah sinkron dari server, kolom `photo`/`thumb` bisa
  berisi URL → thumbnail tampil sebagai kotak inisial (perlu fitur unduh-cache, lihat daftar pekerjaan).
- Nomor transaksi (`OUT-YYMMDD-0001`) dihitung per HP; dua HP bisa membuat nomor sama di hari yang sama.
  Server boleh memberi nomor resmi; atau tambahkan kode perangkat di nomor.
- `cargo-apk` 0.10 sudah tidak aktif dikembangkan; alternatif bila bermasalah: `xbuild` atau
  proyek Gradle kecil yang memuat `libbakadai_lite.so`.

## 7. Status & daftar pekerjaan berikutnya

**Sudah jalan**: semua menu di README; offline-first + antrean sinkron; kompres foto; CSV; cetak ESC/POS
jaringan; scanner barcode mode keyboard; staff/barang/riwayat/biaya tanpa batas; APK rilis bertanda tangan.

**Belum (prioritas disarankan)**
1. Uji di HP sungguhan (keyboard, rotasi layar, tombol kembali Android, performa) — **paling penting**.
2. Pilih foto dari galeri / kamera (JNI ke `Intent.ACTION_PICK` / `ACTION_IMAGE_CAPTURE`); kompresnya sudah ada di `imgproc.rs`.
3. Scan barcode pakai kamera (CameraX + ML Kit lewat JNI, atau decoder Rust `rxing`).
4. Printer Bluetooth (JNI `BluetoothSocket` SPP, kirim byte dari `escpos::to_escpos`).
5. Bagikan struk/CSV ke WhatsApp (Intent `ACTION_SEND` + FileProvider).
6. Login pengguna & banyak bisnis di HP (sekarang satu bisnis per HP; server sudah dirancang multi-bisnis).
7. Jam kerja staff, transfer stok antar bisnis, katalog online, label barcode (ada di aplikasi asli; kontrak API sudah disiapkan).
8. Unduh & cache foto dari server setelah sinkron.
9. PIN per staff saat ganti pengguna di HP yang sama.
10. Ikon aplikasi (tambahkan `resources` di `[package.metadata.android]` berisi `mipmap-*/ic_launcher.png`).

**Bukti dari aplikasi asli**: kode utama aplikasi asli (Dart, `libapp.so`) tidak ada di APK yang diberikan,
jadi aturan diambil dari teks antarmukanya dan dari kode Android yang bisa dibongkar (lihat `MECHANICS.md`).
Nilai bertanda **perkiraan** di `MECHANICS.md` boleh disesuaikan.
