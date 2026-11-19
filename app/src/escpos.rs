//! Struk teks + perintah ESC/POS untuk printer thermal 58 mm (32 kolom) / 80 mm (48 kolom).
//! Pencetakan lewat jaringan (Wi-Fi/LAN, port 9100) memakai TCP biasa — jalan di Android tanpa
//! kode Java. Printer Bluetooth lewat `bt.rs` (JNI).

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

pub fn columns_for_paper(paper_mm: u32) -> usize {
    if paper_mm >= 80 {
        48
    } else {
        32
    }
}

/// Satu baris kiri-kanan dengan titik-titik pengisi yang pas lebar kertas.
pub fn lr(left: &str, right: &str, width: usize) -> String {
    let l: String = left.chars().take(width.saturating_sub(right.chars().count() + 1)).collect();
    let pad = width.saturating_sub(l.chars().count() + right.chars().count());
    format!("{}{}{}", l, " ".repeat(pad), right)
}

pub fn center(s: &str, width: usize) -> String {
    let s: String = s.chars().take(width).collect();
    let pad = (width - s.chars().count()) / 2;
    format!("{}{}", " ".repeat(pad), s)
}

pub fn rule(width: usize) -> String {
    "-".repeat(width)
}

/// Ubah teks struk menjadi byte ESC/POS: init, teks, potong kertas, opsional buka laci kasir.
pub fn to_escpos(text: &str, open_drawer: bool) -> Vec<u8> {
    let mut b: Vec<u8> = vec![0x1B, 0x40]; // ESC @ (init)
    b.extend_from_slice(&[0x1B, 0x74, 0x10]); // ESC t 16 → CP1252
    for ch in text.chars() {
        match ch {
            '\n' => b.push(b'\n'),
            c if (c as u32) < 0x80 => b.push(c as u8),
            'é' => b.push(0xE9),
            'è' => b.push(0xE8),
            _ => b.push(b'?'),
        }
    }
    b.extend_from_slice(b"\n\n\n");
    b.extend_from_slice(&[0x1D, 0x56, 0x42, 0x00]); // GS V B 0 (potong sebagian)
    if open_drawer {
        b.extend_from_slice(&[0x1B, 0x70, 0x00, 0x19, 0xFA]); // ESC p 0 25 250 (kick laci)
    }
    b
}

/// Cetak ke printer Bluetooth (alamat AA:BB:CC:DD:EE:FF) atau jaringan (IP[:port]).
pub fn print(addr: &str, data: &[u8]) -> Result<(), String> {
    if crate::bt::is_bt_addr(addr) {
        crate::bt::print(addr, data)
    } else {
        print_network(addr, data)
    }
}

pub fn print_network(addr: &str, data: &[u8]) -> Result<(), String> {
    let addr = if addr.contains(':') { addr.to_string() } else { format!("{}:9100", addr) };
    let sock = addr.parse().map_err(|_| format!("Alamat printer tidak valid: {}", addr))?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_secs(4)).map_err(|e| format!("Gagal menghubungkan printer: {}", e))?;
    s.set_write_timeout(Some(Duration::from_secs(6))).ok();
    s.write_all(data).map_err(|e| format!("Gagal mengirim data ke printer: {}", e))?;
    Ok(())
}
