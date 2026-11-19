//! Pratinjau desktop (dan dipakai untuk cek otomatis). Di Android, titik masuknya `android_main` di lib.rs.
//! Pemakaian: `bakadai_lite` biasa, atau `bakadai_lite --screenshot gambar.png [--screen kasir]`.

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut shot: Option<std::path::PathBuf> = None;
    let mut screen = String::from("home");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" if i + 1 < args.len() => {
                shot = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--screen" if i + 1 < args.len() => {
                screen = args[i + 1].clone();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    // Ukuran seperti HP (portrait). Untuk gambar cek pakai data contoh di folder sementara.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Bakadai Lite")
        .with_inner_size([412.0, 860.0])
        .with_min_inner_size([340.0, 560.0]);
    let data_dir = if shot.is_some() {
        let d = std::env::temp_dir().join(format!("bakadai-lite-shot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    } else {
        bakadai_lite::desktop_data_dir()
    };

    let mut options = eframe::NativeOptions::default();
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
    }
    options.viewport = viewport;
    options.run_and_return = true;

    let shot_arg = shot.map(|p| (p, screen));
    eframe::run_native(
        "Bakadai Lite",
        options,
        Box::new(move |cc| Ok(Box::new(bakadai_lite::ui::BakadaiApp::new(cc, data_dir, shot_arg)))),
    )
}
