//! Sinkron offline-first ke REST API milik Anda (kontrak lengkap di API_CONTRACT.md).
//! Aplikasi tetap jalan penuh tanpa internet; perubahan antre di `outbox` lalu dikirim
//! bertahap (batch) di thread terpisah agar layar tidak pernah macet.

use crate::db::{now_ms, Db};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub const PUSH_BATCH: i64 = 200;
pub const PULL_LIMIT: i64 = 500;
pub const TIMEOUT_SECS: u64 = 20;

#[derive(Clone, Debug, Default)]
pub struct SyncConfig {
    pub base_url: String,
    pub token: String,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub enum SyncResult {
    Ok { pushed: usize, pulled: usize, photos: usize },
    LoggedIn { token: String, name: String },
    Health(String),
    Err(String),
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(TIMEOUT_SECS)).build()
}

fn url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|x| x.get("message")).and_then(|m| m.as_str()).map(String::from))
                .unwrap_or(body);
            match code {
                401 => "Sesi berakhir, silakan masuk ulang (401)".into(),
                403 => "Anda tidak memiliki akses fitur ini. (403)".into(),
                _ => format!("Server {}: {}", code, msg.chars().take(160).collect::<String>()),
            }
        }
        ureq::Error::Transport(t) => format!("Tidak ada koneksi internet / server tidak terjangkau ({})", t.kind()),
    }
}

fn run_in_thread(ctx: egui::Context, f: impl FnOnce() -> SyncResult + Send + 'static) -> Receiver<SyncResult> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = f();
        let _ = tx.send(r);
        ctx.request_repaint();
    });
    rx
}

pub fn health(ctx: egui::Context, cfg: SyncConfig) -> Receiver<SyncResult> {
    run_in_thread(ctx, move || match agent().get(&url(&cfg.base_url, "/v1/health")).call() {
        Ok(r) => SyncResult::Health(format!("Terhubung (HTTP {})", r.status())),
        Err(e) => SyncResult::Err(err_text(e)),
    })
}

pub fn login(ctx: egui::Context, cfg: SyncConfig, email: String, password: String) -> Receiver<SyncResult> {
    run_in_thread(ctx, move || {
        let body = json!({ "email": email, "password": password, "device_id": cfg.device_id });
        match agent().post(&url(&cfg.base_url, "/v1/auth/login")).send_json(body) {
            Ok(r) => match r.into_json::<Value>() {
                Ok(v) => {
                    let token = v.get("token").and_then(|t| t.as_str()).unwrap_or_default().to_string();
                    let name = v.pointer("/user/name").and_then(|t| t.as_str()).unwrap_or_default().to_string();
                    if token.is_empty() {
                        SyncResult::Err("Respons server tidak valid (token kosong)".into())
                    } else {
                        SyncResult::LoggedIn { token, name }
                    }
                }
                Err(e) => SyncResult::Err(format!("Respons server tidak valid: {}", e)),
            },
            Err(e) => SyncResult::Err(err_text(e)),
        }
    })
}

/// Kirim antrian (push) lalu ambil perubahan dari server (pull).
pub fn sync_now(ctx: egui::Context, data_dir: PathBuf, cfg: SyncConfig) -> Receiver<SyncResult> {
    run_in_thread(ctx, move || match do_sync(&data_dir, &cfg) {
        Ok((pushed, pulled, photos)) => SyncResult::Ok { pushed, pulled, photos },
        Err(e) => SyncResult::Err(e),
    })
}

fn do_sync(dir: &PathBuf, cfg: &SyncConfig) -> Result<(usize, usize, usize), String> {
    if cfg.base_url.trim().is_empty() {
        return Err("Isi alamat server API dulu di Pengaturan".into());
    }
    let db = Db::open(dir)?;
    let a = agent();
    let auth = format!("Bearer {}", cfg.token);
    let mut pushed = 0;
    let mut photos = 0;
    loop {
        let batch = db.outbox_batch(PUSH_BATCH)?;
        if batch.is_empty() {
            break;
        }
        let mut changes = vec![];
        let mut last_seq = 0;
        for (seq, entity, id, op, payload, ts) in &batch {
            last_seq = *seq;
            if entity == "photo" {
                let path = payload.get("path").and_then(|p| p.as_str()).unwrap_or_default();
                if let Ok(bytes) = std::fs::read(path) {
                    a.put(&url(&cfg.base_url, &format!("/v1/products/{}/photo", id)))
                        .set("Authorization", &auth)
                        .set("Content-Type", "image/jpeg")
                        .set("Idempotency-Key", &format!("{}-photo-{}", cfg.device_id, seq))
                        .send_bytes(&bytes)
                        .map_err(err_text)?;
                    photos += 1;
                }
                continue;
            }
            changes.push(json!({ "seq": seq, "entity": entity, "id": id, "op": op, "data": payload, "client_ts": ts }));
        }
        if !changes.is_empty() {
            let n = changes.len();
            let body = json!({ "device_id": cfg.device_id, "changes": changes });
            a.post(&url(&cfg.base_url, "/v1/sync/push"))
                .set("Authorization", &auth)
                .set("Idempotency-Key", &format!("{}-{}", cfg.device_id, last_seq))
                .send_json(body)
                .map_err(err_text)?;
            pushed += n;
        }
        db.outbox_ack(last_seq)?;
    }
    let mut pulled = 0;
    let mut cursor = db.setting("sync_cursor", "0");
    loop {
        let resp: Value = a
            .get(&url(&cfg.base_url, "/v1/sync/pull"))
            .set("Authorization", &auth)
            .query("since", &cursor)
            .query("limit", &PULL_LIMIT.to_string())
            .query("device_id", &cfg.device_id)
            .call()
            .map_err(err_text)?
            .into_json()
            .map_err(|e| format!("Respons server tidak valid: {}", e))?;
        if let Some(arr) = resp.get("changes").and_then(|c| c.as_array()) {
            for ch in arr {
                let entity = ch.get("entity").and_then(|x| x.as_str()).unwrap_or_default();
                let mut data = ch.get("data").cloned().unwrap_or(Value::Null);
                if ch.get("op").and_then(|x| x.as_str()) == Some("delete") {
                    if let Value::Object(m) = &mut data {
                        m.insert("deleted".into(), json!(1));
                    }
                }
                if db.apply_remote(entity, &data)? {
                    pulled += 1;
                }
            }
        }
        if let Some(c) = resp.get("cursor") {
            cursor = c.as_str().map(String::from).unwrap_or_else(|| c.to_string());
            db.set_setting("sync_cursor", &cursor)?;
        }
        if !resp.get("has_more").and_then(|x| x.as_bool()).unwrap_or(false) {
            break;
        }
    }
    db.set_setting("last_sync", &now_ms().to_string())?;
    Ok((pushed, pulled, photos))
}
