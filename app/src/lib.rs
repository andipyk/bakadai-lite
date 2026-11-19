//! Bakadai Lite — aplikasi stok & kasir ringan, offline-first.
//! Satu kode untuk Android (lib `cdylib`, lewat `android_main`) dan komputer (pratinjau / cek).

pub mod bt;
pub mod db;
pub mod escpos;
pub mod imgproc;
pub mod sync;
pub mod ui;

use std::path::PathBuf;

/// Folder data di komputer (Android memakai folder internal aplikasi).
pub fn desktop_data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("BAKADAI_LITE_DATA") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(h) = std::env::var("HOME") {
            return PathBuf::from(h).join("Library/Application Support/BakadaiLite");
        }
    }
    #[cfg(windows)]
    {
        if let Ok(a) = std::env::var("APPDATA") {
            return PathBuf::from(a).join("BakadaiLite");
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local/share/bakadai-lite");
    }
    PathBuf::from("bakadai-lite-data")
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit_android::AndroidApp) {
    use eframe::NativeOptions;
    let dir = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/bakadai-lite"));
    // Sama seperti aplikasi asli (getRegionCode): negara dari konfigurasi bahasa sistem.
    let options = NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native(
        "Bakadai Lite",
        options,
        Box::new(move |cc| Ok(Box::new(ui::BakadaiApp::new(cc, dir, None)))),
    );
}

#[cfg(target_os = "android")]
use winit::platform::android::activity as winit_android;

/// Dipanggil android-activity di thread UI Java, di dalam `Activity.onCreate` (juga tiap rotasi layar).
/// Layar penuh tepi-ke-tepi (sama seperti paksaan Android 15+) dengan bar status & navigasi tetap tampil;
/// tinggi bar dibaca lewat [`system_insets`] dan dipakai sebagai safe area.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_on_create(state: &winit_android::OnCreateState) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| android_ui::edge_to_edge(state)));
}

/// Tinggi bar status (atas) dan bar navigasi (bawah) dalam piksel fisik; `None` bila belum tersedia.
#[cfg(target_os = "android")]
pub fn system_insets() -> Option<(f32, f32)> {
    android_ui::insets()
}
#[cfg(not(target_os = "android"))]
pub fn system_insets() -> Option<(f32, f32)> {
    Some((0.0, 0.0))
}

/// Keluar ke layar utama HP (aplikasi tetap hidup di latar, seperti tombol kembali Android 12+).
#[cfg(target_os = "android")]
pub fn leave_app(_ctx: &eframe::egui::Context) {
    android_ui::move_task_to_back();
}
#[cfg(not(target_os = "android"))]
pub fn leave_app(ctx: &eframe::egui::Context) {
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
}

#[cfg(target_os = "android")]
mod android_ui {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str, Env, JavaVM};

    // LAYOUT_STABLE | LAYOUT_HIDE_NAVIGATION | LAYOUT_FULLSCREEN | LIGHT_STATUS_BAR | LIGHT_NAVIGATION_BAR (ikon gelap).
    const FLAGS: i32 = 0x0100 | 0x0200 | 0x0400 | 0x2000 | 0x0010;

    fn decor<'l>(env: &mut Env<'l>, activity: &JObject) -> jni::errors::Result<(JObject<'l>, JObject<'l>)> {
        let window = env
            .call_method(activity, jni_str!("getWindow"), jni_sig!("()Landroid/view/Window;"), &[])?
            .l()?;
        let decor = env
            .call_method(&window, jni_str!("getDecorView"), jni_sig!("()Landroid/view/View;"), &[])?
            .l()?;
        Ok((window, decor))
    }

    fn with_activity<T>(f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>) -> Option<T> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
        let raw = ctx.context() as jni::sys::jobject;
        vm.attach_current_thread(|env| {
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
            let r = f(env, &activity);
            if r.is_err() {
                let _ = env.exception_clear(); // jangan biarkan exception Java menggantung ke panggilan JNI berikutnya
            }
            r
        })
        .map_err(|e| eprintln!("bakadai-lite: JNI gagal: {e:?}"))
        .ok()
    }

    pub fn edge_to_edge(state: &super::winit_android::OnCreateState) {
        let vm = unsafe { JavaVM::from_raw(state.vm_as_ptr().cast()) };
        let raw = state.activity_as_ptr() as jni::sys::jobject;
        let r = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
            let (window, decor) = decor(env, &activity)?;
            env.call_method(&decor, jni_str!("setSystemUiVisibility"), jni_sig!("(I)V"), &[JValue::Int(FLAGS)])?;
            env.call_method(&window, jni_str!("setStatusBarColor"), jni_sig!("(I)V"), &[JValue::Int(0)])?;
            env.call_method(&window, jni_str!("setNavigationBarColor"), jni_sig!("(I)V"), &[JValue::Int(0)])?;
            Ok(())
        });
        if let Err(e) = r {
            eprintln!("bakadai-lite: gagal set tepi-ke-tepi: {e:?}");
        }
    }

    /// Tinggi bar status & navigasi (px fisik). Utama: WindowInsets dari jendela; cadangan: ukuran bawaan
    /// sistem `status_bar_height`/`navigation_bar_height` (selalu ada, juga sebelum jendela siap).
    pub fn insets() -> Option<(f32, f32)> {
        with_activity(|env, activity| {
            let from_window = window_insets(env, activity).unwrap_or_else(|_| {
                let _ = env.exception_clear();
                None
            });
            match from_window {
                Some((t, b)) if t > 0 => Ok(Some((t as f32, b as f32))),
                _ => {
                    let t = system_dimen(env, activity, "status_bar_height")?;
                    let b = system_dimen(env, activity, "navigation_bar_height")?;
                    Ok(Some((t as f32, b as f32)))
                }
            }
        })
        .flatten()
    }

    fn window_insets(env: &mut Env, activity: &JObject) -> jni::errors::Result<Option<(i32, i32)>> {
        let (_, decor) = decor(env, activity)?;
        let ins = env
            .call_method(&decor, jni_str!("getRootWindowInsets"), jni_sig!("()Landroid/view/WindowInsets;"), &[])?
            .l()?;
        if ins.is_null() {
            return Ok(None); // view belum menempel ke jendela
        }
        let sdk = env.get_static_field(jni_str!("android/os/Build$VERSION"), jni_str!("SDK_INT"), jni_sig!("I"))?.i()?;
        if sdk >= 30 {
            let types = env
                .call_static_method(jni_str!("android/view/WindowInsets$Type"), jni_str!("systemBars"), jni_sig!("()I"), &[])?
                .i()?;
            let i = env
                .call_method(&ins, jni_str!("getInsets"), jni_sig!("(I)Landroid/graphics/Insets;"), &[JValue::Int(types)])?
                .l()?;
            let top = env.get_field(&i, jni_str!("top"), jni_sig!("I"))?.i()?;
            let bottom = env.get_field(&i, jni_str!("bottom"), jni_sig!("I"))?.i()?;
            return Ok(Some((top, bottom)));
        }
        let top = env.call_method(&ins, jni_str!("getSystemWindowInsetTop"), jni_sig!("()I"), &[])?.i()?;
        let bottom = env.call_method(&ins, jni_str!("getSystemWindowInsetBottom"), jni_sig!("()I"), &[])?.i()?;
        Ok(Some((top, bottom)))
    }

    fn system_dimen(env: &mut Env, activity: &JObject, name: &str) -> jni::errors::Result<i32> {
        let res = env
            .call_method(activity, jni_str!("getResources"), jni_sig!("()Landroid/content/res/Resources;"), &[])?
            .l()?;
        let (n, t, p) = (env.new_string(name)?, env.new_string("dimen")?, env.new_string("android")?);
        let id = env
            .call_method(
                &res,
                jni_str!("getIdentifier"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I"),
                &[JValue::Object(&n), JValue::Object(&t), JValue::Object(&p)],
            )?
            .i()?;
        if id == 0 {
            return Ok(0);
        }
        env.call_method(&res, jni_str!("getDimensionPixelSize"), jni_sig!("(I)I"), &[JValue::Int(id)])?.i()
    }

    pub fn move_task_to_back() {
        with_activity(|env, activity| {
            env.call_method(activity, jni_str!("moveTaskToBack"), jni_sig!("(Z)Z"), &[JValue::Bool(true)])?;
            Ok(())
        });
    }
}
