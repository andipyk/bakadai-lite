//! Scan barcode lewat kamera HP: Android NDK Camera2 (tanpa Java) + dekoder rxing (port ZXing).
//! Hanya kanal Y (abu-abu) dari YUV_420_888 yang dipakai, cukup untuk barcode dan juga untuk pratinjau.

use rxing::{BarcodeFormat, DecodeHints};
use std::collections::HashSet;

/// Satu bingkai kamera abu-abu yang sudah diputar tegak (sesuai HP dipegang potret).
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub luma: Vec<u8>,
}

/// Bagian tengah bingkai (40% tinggi) yang dibaca dekoder; sama dengan kotak bidik di layar.
pub const BAND_TOP: f32 = 0.3;
pub const BAND_H: f32 = 0.4;

/// Potong kotak bidik dari bingkai: (abu-abu, lebar, tinggi).
pub fn band(f: &Frame) -> (Vec<u8>, u32, u32) {
    let top = (f.h as f32 * BAND_TOP) as usize;
    let h = (f.h as f32 * BAND_H) as usize;
    (f.luma[top * f.w..(top + h) * f.w].to_vec(), f.w as u32, h as u32)
}

/// Baca barcode di gambar abu-abu. Format barang ritel (EAN-13/Code 128/39) + QR. ITF dan Codabar sengaja
/// tidak dipakai: keduanya sering "membaca" kode palsu dari pola acak di depan kamera.
pub fn decode(luma: &[u8], w: u32, h: u32) -> Option<String> {
    // Dekoder tidak boleh membuat aplikasi tertutup bila gagal di gambar yang aneh.
    std::panic::catch_unwind(|| decode_inner(luma, w, h)).ok().flatten()
}

fn decode_inner(luma: &[u8], w: u32, h: u32) -> Option<String> {
    let formats: HashSet<BarcodeFormat> = [
        // EAN-8 dan UPC-E sengaja tidak dipakai: keduanya bisa "cocok" di separuh barcode EAN-13 sehingga hasilnya
        // hanya sebagian angka. UPC-A dibaca oleh EAN-13 sebagai 13 digit (awalan 0), sama seperti kode di kemasan.
        BarcodeFormat::EAN_13,
        BarcodeFormat::CODE_128,
        BarcodeFormat::CODE_39,
        BarcodeFormat::QR_CODE,
    ]
    .into_iter()
    .collect();
    let mut hints = DecodeHints { PossibleFormats: Some(formats), TryHarder: Some(true), ..Default::default() };
    rxing::helpers::detect_in_luma_slice_with_hints(luma, w, h, None, &mut hints)
        .ok()
        .map(|r| crate::db::normalize_scanned_code(r.getText()))
        .filter(|s| !s.is_empty())
}

/// Putar gambar abu-abu searah jarum jam sebesar `deg` (0/90/180/270).
pub fn rotate(src: &[u8], w: usize, h: usize, deg: i32) -> Frame {
    let mut out = vec![0u8; w * h];
    let (ow, oh) = if deg == 90 || deg == 270 { (h, w) } else { (w, h) };
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match deg {
                90 => (h - 1 - y, x),
                180 => (w - 1 - x, h - 1 - y),
                270 => (y, w - 1 - x),
                _ => (x, y),
            };
            out[ny * ow + nx] = src[y * w + x];
        }
    }
    Frame { w: ow, h: oh, luma: out }
}

/// Izin kamera Android: `Granted`, boleh langsung ditanya (`Ask`), atau sudah ditolak sebelumnya (`Rationale`:
/// Android menyarankan menjelaskan alasannya dulu sebelum dialog resmi muncul lagi).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Permission {
    Granted,
    Ask,
    Rationale,
}

#[cfg(not(target_os = "android"))]
pub struct Camera;

#[cfg(not(target_os = "android"))]
impl Camera {
    pub fn open() -> Result<Self, String> {
        Err("Scan kamera hanya tersedia di HP Android. Di komputer pakai scanner USB lalu Enter.".into())
    }
    pub fn frame(&mut self) -> Option<Frame> {
        None
    }
    pub fn has_torch(&self) -> bool {
        false
    }
    pub fn set_torch(&mut self, _on: bool) {}
}
#[cfg(not(target_os = "android"))]
pub fn permission() -> Permission {
    Permission::Granted
}
#[cfg(not(target_os = "android"))]
pub fn request_permission() {}
#[cfg(not(target_os = "android"))]
pub fn open_app_settings() {}

#[cfg(target_os = "android")]
pub use android::{open_app_settings, permission, request_permission, Camera};

#[cfg(target_os = "android")]
mod android {
    use super::Frame;
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str, JavaVM};
    use ndk_sys as ffi;
    use std::ffi::CStr;
    use std::os::raw::{c_int, c_void};
    use std::ptr::null_mut;

    #[link(name = "camera2ndk")]
    extern "C" {}

    // 1280×720 (didukung hampir semua HP): garis barcode lebih tajam daripada 640×480.
    const W: i32 = 1280;
    const H: i32 = 720;
    const TAG_LENS_FACING: u32 = 524293;
    const TAG_SENSOR_ORIENTATION: u32 = 917518;
    const TAG_AF_MODE: u32 = 65543;
    const LENS_FACING_BACK: u8 = 1;
    const AF_CONTINUOUS_PICTURE: u8 = 4;
    const TAG_FLASH_MODE: u32 = 262146;
    const TAG_FLASH_AVAILABLE: u32 = 327680;
    const FLASH_TORCH: u8 = 2;
    const FLASH_OFF: u8 = 0;
    const YUV_420_888: i32 = 35;

    unsafe extern "C" fn dev_cb(_: *mut c_void, _: *mut ffi::ACameraDevice) {}
    unsafe extern "C" fn dev_err(_: *mut c_void, _: *mut ffi::ACameraDevice, _: c_int) {}
    unsafe extern "C" fn sess_cb(_: *mut c_void, _: *mut ffi::ACameraCaptureSession) {}

    pub struct Camera {
        manager: *mut ffi::ACameraManager,
        device: *mut ffi::ACameraDevice,
        reader: *mut ffi::AImageReader,
        session: *mut ffi::ACameraCaptureSession,
        request: *mut ffi::ACaptureRequest,
        target: *mut ffi::ACameraOutputTarget,
        container: *mut ffi::ACaptureSessionOutputContainer,
        output: *mut ffi::ACaptureSessionOutput,
        // Kamera memanggil struct callback ini selama hidup, jadi alamatnya harus tetap (Box).
        _dev_cbs: Box<ffi::ACameraDevice_StateCallbacks>,
        _sess_cbs: Box<ffi::ACameraCaptureSession_stateCallbacks>,
        rotation: i32,
        torch: bool,
        buf: Vec<u8>,
    }

    fn with_activity<T>(f: impl FnOnce(&mut jni::Env, &JObject) -> jni::errors::Result<T>) -> Option<T> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
        let raw = ctx.context() as jni::sys::jobject;
        vm.attach_current_thread(|env| {
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
            let r = f(env, &activity);
            if r.is_err() {
                let _ = env.exception_clear();
            }
            r
        })
        .ok()
    }

    pub fn permission() -> super::Permission {
        with_activity(|env, activity| {
            let perm = env.new_string("android.permission.CAMERA")?;
            let granted = env
                .call_method(activity, jni_str!("checkSelfPermission"), jni_sig!("(Ljava/lang/String;)I"), &[JValue::Object(&perm)])?
                .i()?;
            if granted == 0 {
                return Ok(super::Permission::Granted);
            }
            let rationale = env
                .call_method(activity, jni_str!("shouldShowRequestPermissionRationale"), jni_sig!("(Ljava/lang/String;)Z"), &[JValue::Object(&perm)])?
                .z()?;
            Ok(if rationale { super::Permission::Rationale } else { super::Permission::Ask })
        })
        .unwrap_or(super::Permission::Ask)
    }

    /// Munculkan dialog izin resmi Android (tidak bisa diubah isinya oleh aplikasi).
    pub fn request_permission() {
        with_activity(|env, activity| {
            let perm = env.new_string("android.permission.CAMERA")?;
            let arr = env.new_object_array(1, jni_str!("java/lang/String"), &perm)?;
            env.call_method(activity, jni_str!("requestPermissions"), jni_sig!("([Ljava/lang/String;I)V"), &[JValue::Object(&arr), JValue::Int(8)])?;
            Ok(())
        });
    }

    /// Halaman info aplikasi di Pengaturan Android (Izin → Kamera).
    pub fn open_app_settings() {
        with_activity(|env, activity| {
            let action = env.new_string("android.settings.APPLICATION_DETAILS_SETTINGS")?;
            let pkg = env.call_method(activity, jni_str!("getPackageName"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
            let scheme = env.new_string("package")?;
            let uri = env
                .call_static_method(
                    jni_str!("android/net/Uri"),
                    jni_str!("fromParts"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;"),
                    &[JValue::Object(&scheme), JValue::Object(&pkg), JValue::Object(&JObject::null())],
                )?
                .l()?;
            let intent = env.new_object(
                jni_str!("android/content/Intent"),
                jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
                &[JValue::Object(&action), JValue::Object(&uri)],
            )?;
            env.call_method(activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)])?;
            Ok(())
        });
    }

    fn ok(status: ffi::camera_status_t, what: &str) -> Result<(), String> {
        if status.0 == 0 {
            Ok(())
        } else {
            Err(format!("Kamera gagal dibuka ({what}: {})", status.0))
        }
    }

    impl Camera {
        pub fn open() -> Result<Self, String> {
            unsafe {
                let mut cam = Camera {
                    manager: ffi::ACameraManager_create(),
                    device: null_mut(),
                    reader: null_mut(),
                    session: null_mut(),
                    request: null_mut(),
                    target: null_mut(),
                    container: null_mut(),
                    output: null_mut(),
                    _dev_cbs: Box::new(ffi::ACameraDevice_StateCallbacks {
                        context: null_mut(),
                        onDisconnected: Some(dev_cb),
                        onError: Some(dev_err),
                    }),
                    _sess_cbs: Box::new(ffi::ACameraCaptureSession_stateCallbacks {
                        context: null_mut(),
                        onClosed: Some(sess_cb),
                        onReady: Some(sess_cb),
                        onActive: Some(sess_cb),
                    }),
                    rotation: 90,
                    torch: false,
                    buf: vec![0; (W * H) as usize],
                };
                // Drop menutup apa pun yang sudah terbuka bila langkah berikut gagal.
                cam.start()?;
                Ok(cam)
            }
        }

        unsafe fn start(&mut self) -> Result<(), String> {
            let mut list: *mut ffi::ACameraIdList = null_mut();
            ok(ffi::ACameraManager_getCameraIdList(self.manager, &mut list), "daftar kamera")?;
            let mut chosen: Option<std::ffi::CString> = None;
            for i in 0..(*list).numCameras as usize {
                let id = *(*list).cameraIds.add(i);
                let mut meta: *mut ffi::ACameraMetadata = null_mut();
                if ffi::ACameraManager_getCameraCharacteristics(self.manager, id, &mut meta).0 != 0 {
                    continue;
                }
                let mut e: ffi::ACameraMetadata_const_entry = std::mem::zeroed();
                let back = ffi::ACameraMetadata_getConstEntry(meta, TAG_LENS_FACING, &mut e).0 == 0
                    && e.count > 0
                    && *e.data.u8_ == LENS_FACING_BACK;
                if back || chosen.is_none() {
                    if ffi::ACameraMetadata_getConstEntry(meta, TAG_SENSOR_ORIENTATION, &mut e).0 == 0 && e.count > 0 {
                        self.rotation = *e.data.i32_;
                    }
                    self.torch = ffi::ACameraMetadata_getConstEntry(meta, TAG_FLASH_AVAILABLE, &mut e).0 == 0
                        && e.count > 0
                        && *e.data.u8_ == 1;
                    chosen = Some(CStr::from_ptr(id).to_owned());
                }
                ffi::ACameraMetadata_free(meta);
                if back {
                    break;
                }
            }
            ffi::ACameraManager_deleteCameraIdList(list);
            let id = chosen.ok_or("HP ini tidak punya kamera")?;

            if ffi::AImageReader_new(W, H, YUV_420_888, 2, &mut self.reader).0 != 0 {
                return Err("Kamera gagal dibuka (pembaca gambar)".into());
            }
            let mut window: *mut ffi::ANativeWindow = null_mut();
            if ffi::AImageReader_getWindow(self.reader, &mut window).0 != 0 {
                return Err("Kamera gagal dibuka (jendela gambar)".into());
            }
            ok(
                ffi::ACameraManager_openCamera(self.manager, id.as_ptr(), &mut *self._dev_cbs, &mut self.device),
                "buka kamera",
            )?;
            ok(ffi::ACaptureSessionOutputContainer_create(&mut self.container), "wadah")?;
            ok(ffi::ACaptureSessionOutput_create(window, &mut self.output), "output")?;
            ok(ffi::ACaptureSessionOutputContainer_add(self.container, self.output), "output")?;
            ok(ffi::ACameraDevice_createCaptureSession(self.device, self.container, &*self._sess_cbs, &mut self.session), "sesi")?;
            ok(ffi::ACameraDevice_createCaptureRequest(self.device, ffi::ACameraDevice_request_template(1), &mut self.request), "permintaan")?;
            ok(ffi::ACameraOutputTarget_create(window, &mut self.target), "target")?;
            ok(ffi::ACaptureRequest_addTarget(self.request, self.target), "target")?;
            let af = AF_CONTINUOUS_PICTURE;
            let _ = ffi::ACaptureRequest_setEntry_u8(self.request, TAG_AF_MODE, 1, &af);
            let mut req = self.request;
            ok(ffi::ACameraCaptureSession_setRepeatingRequest(self.session, null_mut(), 1, &mut req, null_mut()), "mulai")?;
            Ok(())
        }

        /// HP punya lampu kilat yang bisa dinyalakan terus (mode senter).
        pub fn has_torch(&self) -> bool {
            self.torch
        }

        /// Senter untuk scan di tempat gelap: ubah FLASH_MODE lalu kirim ulang permintaan berulang.
        pub fn set_torch(&mut self, on: bool) {
            if !self.torch || self.session.is_null() {
                return;
            }
            unsafe {
                let mode = if on { FLASH_TORCH } else { FLASH_OFF };
                let _ = ffi::ACaptureRequest_setEntry_u8(self.request, TAG_FLASH_MODE, 1, &mode);
                let mut req = self.request;
                let _ = ffi::ACameraCaptureSession_setRepeatingRequest(self.session, null_mut(), 1, &mut req, null_mut());
            }
        }

        /// Bingkai terbaru (sudah tegak), atau None bila belum ada bingkai baru.
        pub fn frame(&mut self) -> Option<Frame> {
            unsafe {
                let mut img: *mut ffi::AImage = null_mut();
                if ffi::AImageReader_acquireLatestImage(self.reader, &mut img).0 != 0 || img.is_null() {
                    return None;
                }
                let (mut data, mut len, mut stride) = (null_mut::<u8>(), 0 as c_int, 0i32);
                let got = ffi::AImage_getPlaneData(img, 0, &mut data, &mut len).0 == 0
                    && ffi::AImage_getPlaneRowStride(img, 0, &mut stride).0 == 0
                    && !data.is_null();
                if got {
                    let (w, h, stride) = (W as usize, H as usize, stride as usize);
                    let plane = std::slice::from_raw_parts(data, len as usize);
                    for y in 0..h {
                        let row = &plane[y * stride..(y * stride + w).min(plane.len())];
                        self.buf[y * w..y * w + row.len()].copy_from_slice(row);
                    }
                }
                ffi::AImage_delete(img);
                got.then(|| super::rotate(&self.buf, W as usize, H as usize, self.rotation))
            }
        }
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            unsafe {
                if !self.session.is_null() {
                    // Hentikan aliran gambar dulu sebelum sesi, kamera, dan pembaca gambar dilepas.
                    let _ = ffi::ACameraCaptureSession_stopRepeating(self.session);
                    ffi::ACameraCaptureSession_close(self.session);
                }
                if !self.device.is_null() {
                    ffi::ACameraDevice_close(self.device);
                }
                if !self.request.is_null() {
                    ffi::ACaptureRequest_free(self.request);
                }
                if !self.target.is_null() {
                    ffi::ACameraOutputTarget_free(self.target);
                }
                if !self.output.is_null() {
                    ffi::ACaptureSessionOutput_free(self.output);
                }
                if !self.container.is_null() {
                    ffi::ACaptureSessionOutputContainer_free(self.container);
                }
                if !self.reader.is_null() {
                    ffi::AImageReader_delete(self.reader);
                }
                ffi::ACameraManager_delete(self.manager);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_is_middle_rows() {
        let f = Frame { w: 2, h: 10, luma: (0..20).collect() };
        let (l, w, h) = band(&f);
        assert_eq!((w, h), (2, 4));
        assert_eq!(l, vec![6, 7, 8, 9, 10, 11, 12, 13]); // baris 3..7
    }

    #[test]
    fn rotate_quarter_turn() {
        // 3×2: baris "1 2 3" / "4 5 6" diputar 90° searah jarum jam → 2×3: "4 1" / "5 2" / "6 3".
        let f = rotate(&[1, 2, 3, 4, 5, 6], 3, 2, 90);
        assert_eq!((f.w, f.h), (2, 3));
        assert_eq!(f.luma, vec![4, 1, 5, 2, 6, 3]);
    }

    #[test]
    fn decodes_code128_from_luma() {
        // Gambar barcode Code 128 "12345" yang digambar sendiri (pola batang resmi), dibesarkan 3×.
        let pattern = code128_modules("12345");
        let (scale, quiet, h) = (3usize, 20usize, 60usize);
        let w = (pattern.len() + 2 * quiet) * scale;
        let mut img = vec![255u8; w * h];
        for (i, black) in pattern.iter().enumerate() {
            if *black {
                for y in 0..h {
                    for s in 0..scale {
                        img[y * w + (quiet + i) * scale + s] = 0;
                    }
                }
            }
        }
        assert_eq!(decode(&img, w as u32, h as u32).as_deref(), Some("12345"));
    }

    /// Modul Code 128 set B untuk teks pendek (hanya untuk uji).
    fn code128_modules(text: &str) -> Vec<bool> {
        const P: [&str; 107] = [
            "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213", "221312", "231212",
            "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132", "221231", "213212", "223112", "312131",
            "311222", "321122", "321221", "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
            "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121",
            "313121", "211331", "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
            "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
            "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242", "121142", "121241", "114212",
            "124112", "124211", "411212", "421112", "421211", "212141", "214121", "412121", "111143", "111341", "131141", "114113",
            "114311", "411113", "411311", "113141", "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
        ];
        let mut codes = vec![104usize]; // Start B
        codes.extend(text.bytes().map(|b| (b - 32) as usize));
        let check = codes.iter().enumerate().map(|(i, c)| if i == 0 { *c } else { c * i }).sum::<usize>() % 103;
        codes.push(check);
        codes.push(106); // Stop
        let mut out = Vec::new();
        for c in codes {
            for (i, d) in P[c].bytes().enumerate() {
                out.extend(std::iter::repeat_n(i % 2 == 0, (d - b'0') as usize));
            }
        }
        out
    }
}
