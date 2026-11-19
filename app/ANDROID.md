# Membuat APK Android

Kode yang sama dipakai untuk Android dan komputer. Di Android aplikasi dijalankan lewat `android_main` (src/lib.rs),
data disimpan di folder internal aplikasi (SQLite), tanpa Java/Kotlin dan tanpa layanan Google.

## Cara cepat (sudah disiapkan)
APK siap pasang sudah ada: **`app/BakadaiLite.apk`** (3,6 MB, untuk HP Android 7.0+ 64-bit / arm64).
Untuk membuat ulang setelah kode diubah cukup jalankan:
```
sh app/build-apk.sh
```
Skrip ini memakai alat yang terpasang di `app/.tools` (Rust + target Android + cargo-apk; tidak mengubah sistem),
Android SDK/NDK di `~/Library/Android/sdk`, dan Java dari Craft Anything. Kunci tanda tangan dibuat di
`app/.tools/bakadai-lite.keystore`, sandi acak di `app/.tools/bakadai-lite.keystore.pass` (keduanya tidak masuk git) —
**cadangkan keduanya**; update aplikasi harus ditandatangani dengan kunci yang sama. Skrip juga membuat
`BakadaiLite.aab` (format wajib Play Store) lewat `aapt2` + `bundletool` (`app/.tools/bundletool-1.18.3.jar`).

Pasang ke HP: aktifkan "Instal aplikasi tidak dikenal", kirim file APK ke HP (WhatsApp/Drive/kabel) lalu buka;
atau dengan kabel USB: `adb install -r app/BakadaiLite.apk`.

## Cara manual: pasang alat
1. Rust lewat **rustup** (bukan Homebrew) — https://rustup.rs
2. Target Android:
   ```
   rustup target add aarch64-linux-android armv7-linux-androideabi
   ```
3. Java 17 (untuk menandatangani APK), mis. `brew install openjdk@17`.
4. Android SDK + NDK (sudah ada di komputer ini: `~/Library/Android/sdk`, NDK 28.2).
   ```
   export ANDROID_HOME=~/Library/Android/sdk
   export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/28.2.13676358
   ```
5. Alat pembuat APK: `cargo install cargo-apk`

## Buat APK
```
cd app
cargo apk build --release --lib
```
Hasil: `target/release/apk/BakadaiLite.apk` (arm64 + armv7). Pasang ke HP: `adb install -r target/release/apk/BakadaiLite.apk`.

Pengaturan APK (nama paket `dev.harau.bakadailite`, minSdk 24, izin INTERNET) ada di bagian
`[package.metadata.android]` di Cargo.toml (targetSdk 36). Kunci tanda tangan dikirim `build-apk.sh` lewat env
`CARGO_APK_RELEASE_KEYSTORE` / `CARGO_APK_RELEASE_KEYSTORE_PASSWORD`.

## Kenapa lebih ringan dari aplikasi asli
| | Aplikasi asli | Bakadai Lite |
|---|---|---|
| Teknologi | Flutter + 5 DEX (≈23 MB APK dasar, belum termasuk split) + Firebase, OneSignal, Sentry, RevenueCat, ML Kit, Airbridge | Rust native + SQLite, tanpa SDK pihak ketiga |
| Izin | 47 izin (kontak, Bluetooth, lokasi, kamera, iklan…) | 1 izin (internet, hanya untuk sinkron) |
| Data | Harus online (Firebase) | Offline-first, sinkron saat ada internet |
| Foto | Diunggah apa adanya | Dikompres di HP (≤ 1024 px, JPEG 78) sebelum disimpan/diunggah |

## Batasan versi ini di Android (perlu jembatan Android/JNI kecil, bisa ditambah nanti)
- **Pilih foto dari galeri/kamera**: belum; versi ini menerima foto lewat path file / seret ke jendela (komputer). Kompresinya sudah jalan.
- **Scan barcode pakai kamera**: belum; scanner barcode USB/Bluetooth (mode keyboard) sudah didukung di kolom cari.
- **Printer Bluetooth**: sudah (`src/bt.rs`, JNI tanpa Java). Pair printer di Pengaturan Bluetooth HP, lalu Pengaturan > "Pilih printer Bluetooth". Printer Wi-Fi/LAN (port 9100) tetap bisa.
- **Bagikan file CSV / struk ke WhatsApp**: belum; struk bisa disalin, CSV disimpan di folder aplikasi.
- Keyboard layar: egui di Android memakai keyboard sistem lewat winit; bila di HP tertentu keyboard tidak muncul,
  perlu panggilan `showSoftInput` lewat JNI.
