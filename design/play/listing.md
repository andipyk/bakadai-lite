# Bakadai Lite — teks Play Store

**Nama aplikasi (≤30):** Bakadai Lite

**Deskripsi singkat (≤80):**
Kasir & stok barang ringan untuk toko. Jalan tanpa internet, tanpa batas.

**Deskripsi lengkap:**

Bakadai Lite adalah aplikasi kasir dan stok barang untuk warung, toko kelontong, dan usaha kecil. Ringan (sekitar 4 MB), cepat di HP murah, dan tetap jalan tanpa internet.

Kasir
• Pilih barang dari katalog atau scan barcode pakai kamera HP (atau scanner USB/Bluetooth)
• Diskon, biaya tambahan, uang pas, dan kembalian otomatis
• Catat piutang pembeli dan pembayaran sebagian
• Cetak struk ke printer thermal Bluetooth atau Wi-Fi (58/80 mm)

Stok barang
• Stok masuk, stok keluar, dan audit stok
• Pengingat stok rendah, stok habis, dan kedaluwarsa
• Harga modal rata-rata otomatis dan harga grosir bertingkat
• Foto barang dikecilkan otomatis supaya HP tidak cepat penuh

Laporan
• Omset, laba kotor, laba bersih, dan barang terlaris
• Pengeluaran (gaji, listrik, sewa) masuk ke laporan laba
• Ekspor CSV untuk dibuka di Excel

Tanpa batas
• Barang, transaksi, dan staff tanpa batas
• Akses staff bisa diatur per fitur (kasir, gudang, admin)

Data Anda di HP Anda
• Semua data tersimpan di perangkat. Tidak ada iklan dan tidak ada pelacakan.
• Bisa disinkronkan ke server milik Anda sendiri bila diperlukan (opsional).

**Kategori:** Bisnis
**Tag:** Kasir, POS, stok barang, inventaris, UMKM

**Aset:**
- Ikon 512×512: `design/play/icon-512.png`
- Feature graphic 1024×500: `design/play/feature-graphic.png`
- Screenshot ponsel (824×1474): `app/shots/home.png`, `kasir.png`, `barang.png`, `history.png`, `reports.png`, `more.png`

## Data Safety (jawaban sesuai kode, Nov 2026)

- Mengumpulkan data pengguna: **Tidak**. Developer tidak menerima data apa pun; tidak ada analitik, iklan, atau crash reporting.
- Membagikan data ke pihak ketiga: **Tidak**.
- Sinkron opsional mengirim data toko ke server yang **diisi sendiri oleh pengguna** (bukan server developer) melalui HTTPS. Menurut panduan Google, data yang dikirim ke server pilihan pengguna tidak dihitung sebagai "dikumpulkan" oleh developer.
- Data dienkripsi saat transit: **Ya** (HTTPS, bila sinkron dipakai).
- Pengguna bisa meminta data dihapus: data ada di HP; hapus aplikasi atau data aplikasi akan menghapus semuanya.

Izin: `INTERNET` (sinkron opsional), `BLUETOOTH` (≤ Android 11) dan `BLUETOOTH_CONNECT` (printer struk), `CAMERA` (scan barcode; gambar diproses di HP, tidak disimpan/dikirim, jadi tidak dihitung "dikumpulkan").

## Rating konten
Aplikasi bisnis/utilitas: tanpa kekerasan, konten seksual, perjudian, atau interaksi antarpengguna. Target pengguna: 18+ (pemilik/staf toko).
