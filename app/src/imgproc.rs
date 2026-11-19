//! Kompres foto produk sebelum disimpan/diunggah:
//! - putar sesuai EXIF (foto kamera HP sering miring), lalu metadata (lokasi GPS dll.) dibuang;
//! - sisi terpanjang dikecilkan ke 1024 px, disimpan JPEG kualitas 78;
//! - thumbnail 256 px kualitas 70 untuk daftar barang & kasir (hemat memori dan baterai).

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::path::{Path, PathBuf};

pub const PHOTO_MAX_SIDE: u32 = 1024;
pub const PHOTO_QUALITY: u8 = 78;
pub const THUMB_MAX_SIDE: u32 = 256;
pub const THUMB_QUALITY: u8 = 70;
/// "Ukuran file maksimal 10MB" — batas file masukan seperti di aplikasi asli.
pub const MAX_INPUT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Compressed {
    pub photo: PathBuf,
    pub thumb: PathBuf,
    pub original_bytes: u64,
    pub photo_bytes: u64,
    pub thumb_bytes: u64,
    pub width: u32,
    pub height: u32,
}

impl Compressed {
    pub fn saved_percent(&self) -> f64 {
        if self.original_bytes == 0 {
            return 0.0;
        }
        100.0 * (1.0 - self.photo_bytes as f64 / self.original_bytes as f64)
    }
}

fn load_oriented(src: &Path) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(src).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| format!("Format file tidak didukung ({})", e))?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

/// Ukuran tujuan, sama dengan aplikasi asli (image_picker `ImageResizer.calculateTargetSize`):
/// lebar/tinggi dibatasi max tanpa pernah memperbesar, rasio dijaga, dibulatkan.
pub fn calculate_target_size(w: f64, h: f64, max_w: Option<f64>, max_h: Option<f64>) -> (u32, u32) {
    let ratio = w / h;
    let mut tw = max_w.map(|m| w.min(m.round())).unwrap_or(w);
    let mut th = max_h.map(|m| h.min(m.round())).unwrap_or(h);
    let shrink_w = max_w.map(|m| m < w).unwrap_or(false);
    let shrink_h = max_h.map(|m| m < h).unwrap_or(false);
    if shrink_w || shrink_h {
        let down_w = th * ratio;
        let down_h = tw / ratio;
        if down_h > th {
            tw = down_w.round();
        } else {
            th = down_h.round();
        }
    }
    (tw.max(1.0) as u32, th.max(1.0) as u32)
}

/// Faktor perkecil cepat pangkat dua, sama dengan `ImageResizer.calculateSampleSize`:
/// terus digandakan selama (sisi/2)/faktor masih ≥ ukuran tujuan.
pub fn calculate_sample_size(w: u32, h: u32, req_w: u32, req_h: u32) -> u32 {
    let mut s = 1;
    if h > req_h || w > req_w {
        let (hh, hw) = (h / 2, w / 2);
        while hh / s >= req_h && hw / s >= req_w {
            s *= 2;
        }
    }
    s
}

fn shrink(img: &DynamicImage, max_side: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= max_side {
        return img.clone();
    }
    let (tw, th) = calculate_target_size(w as f64, h as f64, Some(max_side as f64), Some(max_side as f64));
    // Langkah cepat dulu (seperti inSampleSize di aplikasi asli): kecilkan dengan faktor pangkat dua
    // memakai filter kotak yang murah, baru haluskan ke ukuran akhir. Hemat CPU & memori di HP.
    let s = calculate_sample_size(w, h, tw, th);
    let base = if s > 1 { DynamicImage::ImageRgba8(image::imageops::thumbnail(img, w / s, h / s)) } else { img.clone() };
    base.resize_exact(tw, th, image::imageops::FilterType::Triangle)
}

/// Gambar dengan latar transparan diberi latar putih lalu tetap dikompres JPEG.
/// (Aplikasi asli melewatkan kompresi untuk PNG transparan: "compressing is not supported for type PNG".)
fn flatten_on_white(img: &DynamicImage) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])])
    })
}

fn write_jpeg(img: &DynamicImage, dst: &Path, quality: u8) -> Result<u64, String> {
    let rgb = flatten_on_white(img);
    let mut buf = Vec::with_capacity((rgb.width() * rgb.height() / 4) as usize);
    JpegEncoder::new_with_quality(&mut buf, quality).encode_image(&rgb).map_err(|e| e.to_string())?;
    std::fs::write(dst, &buf).map_err(|e| e.to_string())?;
    Ok(buf.len() as u64)
}

/// Kompres `src` ke folder `photos_dir` dengan nama dasar `base`.
pub fn compress_photo(src: &Path, photos_dir: &Path, base: &str) -> Result<Compressed, String> {
    let original_bytes = std::fs::metadata(src).map_err(|e| e.to_string())?.len();
    if original_bytes > MAX_INPUT_BYTES {
        return Err("Ukuran file maksimal 10MB.".into());
    }
    std::fs::create_dir_all(photos_dir).map_err(|e| e.to_string())?;
    let img = load_oriented(src)?;
    let big = shrink(&img, PHOTO_MAX_SIDE);
    let photo = photos_dir.join(format!("{}.jpg", base));
    let photo_bytes = write_jpeg(&big, &photo, PHOTO_QUALITY)?;
    let small = shrink(&big, THUMB_MAX_SIDE);
    let thumb = photos_dir.join(format!("{}_t.jpg", base));
    let thumb_bytes = write_jpeg(&small, &thumb, THUMB_QUALITY)?;
    Ok(Compressed { photo, thumb, original_bytes, photo_bytes, thumb_bytes, width: big.width(), height: big.height() })
}

/// Baca thumbnail sebagai gambar egui (dipanggil hanya untuk baris yang terlihat).
pub fn load_thumb(path: &Path) -> Option<egui::ColorImage> {
    let img = image::open(path).ok()?;
    let img = shrink(&img, THUMB_MAX_SIDE).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_size_matches_original_formula() {
        // 4000×3000 dibatasi 1024×1024 → 1024×768 (rasio dijaga)
        assert_eq!(calculate_target_size(4000.0, 3000.0, Some(1024.0), Some(1024.0)), (1024, 768));
        // potret 3000×4000 → 768×1024
        assert_eq!(calculate_target_size(3000.0, 4000.0, Some(1024.0), Some(1024.0)), (768, 1024));
        // tidak pernah diperbesar
        assert_eq!(calculate_target_size(800.0, 600.0, Some(1024.0), Some(1024.0)), (800, 600));
        // tanpa batas → ukuran asli
        assert_eq!(calculate_target_size(800.0, 600.0, None, None), (800, 600));
    }

    #[test]
    fn sample_size_powers_of_two() {
        assert_eq!(calculate_sample_size(4000, 3000, 1024, 768), 2);
        assert_eq!(calculate_sample_size(8000, 6000, 1024, 768), 4);
        assert_eq!(calculate_sample_size(1000, 700, 1024, 768), 1);
    }

    #[test]
    fn transparent_png_becomes_white_jpeg() {
        let dir = std::env::temp_dir().join(format!("kl-alpha-{}", crate::db::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.png");
        image::RgbaImage::from_pixel(50, 50, image::Rgba([0, 0, 0, 0])).save(&src).unwrap();
        let c = compress_photo(&src, &dir.join("photos"), "a").unwrap();
        let out = image::open(&c.photo).unwrap().to_rgb8();
        assert!(out.get_pixel(10, 10).0.iter().all(|v| *v > 240));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compresses_large_png() {
        let dir = std::env::temp_dir().join(format!("kl-img-{}", crate::db::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.png");
        let img = image::RgbImage::from_fn(3000, 2000, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8]));
        img.save(&src).unwrap();
        let c = compress_photo(&src, &dir.join("photos"), "p1").unwrap();
        assert_eq!(c.width.max(c.height), PHOTO_MAX_SIDE);
        assert!(c.photo_bytes < c.original_bytes);
        assert!(c.thumb_bytes < c.photo_bytes);
        let _ = std::fs::remove_dir_all(dir);
    }
}
