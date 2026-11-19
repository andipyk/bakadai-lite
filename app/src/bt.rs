//! Printer thermal Bluetooth (SPP/RFCOMM) lewat JNI ke API Android bawaan — tanpa kode Java/Kotlin.
//! Printer harus sudah di-pair di Pengaturan Bluetooth HP.

/// Alamat Bluetooth "AA:BB:CC:DD:EE:FF" (beda dengan IP:port printer jaringan).
pub fn is_bt_addr(s: &str) -> bool {
    let p: Vec<&str> = s.split(':').collect();
    p.len() == 6 && p.iter().all(|h| h.len() == 2 && h.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(not(target_os = "android"))]
pub fn paired() -> Result<Vec<(String, String)>, String> {
    Err("Printer Bluetooth hanya tersedia di Android".into())
}

#[cfg(not(target_os = "android"))]
pub fn print(_mac: &str, _data: &[u8]) -> Result<(), String> {
    Err("Printer Bluetooth hanya tersedia di Android".into())
}

#[cfg(target_os = "android")]
pub use android::{paired, print};

#[cfg(target_os = "android")]
mod android {
    use jni::objects::{JObject, JObjectArray, JString, JValue};
    use jni::{jni_sig, jni_str, Env, JavaVM};

    const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";

    fn with_env<T>(f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>) -> Result<T, String> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
        let raw_activity = ctx.context() as jni::sys::jobject;
        vm.attach_current_thread(|env| {
            // SAFETY: ndk_context memegang global ref Activity selama aplikasi hidup.
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw_activity)? };
            f(env, &activity)
        })
        .map_err(|e| format!("Printer Bluetooth gagal: pastikan Bluetooth dan printer menyala, dan printer sudah di-pair ({e})"))
    }

    /// Android 12+ butuh izin BLUETOOTH_CONNECT saat dipakai; kalau belum ada, minta lalu suruh coba lagi.
    fn has_permission(env: &mut Env, activity: &JObject) -> jni::errors::Result<bool> {
        let sdk = env
            .get_static_field(jni_str!("android/os/Build$VERSION"), jni_str!("SDK_INT"), jni_sig!("I"))?
            .i()?;
        if sdk < 31 {
            return Ok(true); // izin BLUETOOTH lama diberikan saat instal
        }
        let perm = env.new_string("android.permission.BLUETOOTH_CONNECT")?;
        let granted = env
            .call_method(activity, jni_str!("checkSelfPermission"), jni_sig!("(Ljava/lang/String;)I"), &[JValue::Object(&perm)])?
            .i()?;
        if granted == 0 {
            return Ok(true);
        }
        let arr = env.new_object_array(1, jni_str!("java/lang/String"), &perm)?;
        env.call_method(activity, jni_str!("requestPermissions"), jni_sig!("([Ljava/lang/String;I)V"), &[JValue::Object(&arr), JValue::Int(7)])?;
        Ok(false)
    }

    fn adapter<'l>(env: &mut Env<'l>) -> jni::errors::Result<JObject<'l>> {
        env.call_static_method(
            jni_str!("android/bluetooth/BluetoothAdapter"),
            jni_str!("getDefaultAdapter"),
            jni_sig!("()Landroid/bluetooth/BluetoothAdapter;"),
            &[],
        )?
        .l()
    }

    fn jstr(env: &mut Env, obj: JObject) -> jni::errors::Result<String> {
        if obj.is_null() {
            return Ok(String::new());
        }
        env.cast_local::<JString>(obj)?.try_to_string(env)
    }

    /// Printer yang sudah di-pair: (nama, alamat).
    pub fn paired() -> Result<Vec<(String, String)>, String> {
        let res = with_env(|env, activity| {
            if !has_permission(env, activity)? {
                return Ok(None);
            }
            let ad = adapter(env)?;
            if ad.is_null() {
                return Ok(Some(Err("HP ini tidak punya Bluetooth".to_string())));
            }
            let set = env.call_method(&ad, jni_str!("getBondedDevices"), jni_sig!("()Ljava/util/Set;"), &[])?.l()?;
            if set.is_null() {
                return Ok(Some(Err("Bluetooth mati. Nyalakan Bluetooth lalu coba lagi".to_string())));
            }
            let arr = env.call_method(&set, jni_str!("toArray"), jni_sig!("()[Ljava/lang/Object;"), &[])?.l()?;
            let arr = env.cast_local::<JObjectArray>(arr)?;
            let mut out = Vec::new();
            for i in 0..arr.len(env)? {
                let dev = arr.get_element(env, i)?;
                let name = env.call_method(&dev, jni_str!("getName"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
                let addr = env.call_method(&dev, jni_str!("getAddress"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
                out.push((jstr(env, name)?, jstr(env, addr)?));
            }
            Ok(Some(Ok(out)))
        })?;
        match res {
            None => Err("Izinkan akses Perangkat sekitar (Bluetooth), lalu coba lagi".into()),
            Some(r) => r,
        }
    }

    pub fn print(mac: &str, data: &[u8]) -> Result<(), String> {
        let res = with_env(|env, activity| {
            if !has_permission(env, activity)? {
                return Ok(false);
            }
            let ad = adapter(env)?;
            if ad.is_null() {
                return Err(jni::errors::Error::NullPtr("BluetoothAdapter"));
            }
            let mac = env.new_string(mac)?;
            let dev = env
                .call_method(&ad, jni_str!("getRemoteDevice"), jni_sig!("(Ljava/lang/String;)Landroid/bluetooth/BluetoothDevice;"), &[JValue::Object(&mac)])?
                .l()?;
            let uuid_s = env.new_string(SPP_UUID)?;
            let uuid = env
                .call_static_method(jni_str!("java/util/UUID"), jni_str!("fromString"), jni_sig!("(Ljava/lang/String;)Ljava/util/UUID;"), &[JValue::Object(&uuid_s)])?
                .l()?;
            // Printer murah sering menolak cara standar; coba berurutan: standar, insecure, channel 1 langsung.
            let mut last = Ok(());
            for attempt in 0..3 {
                let sock = match attempt {
                    0 => env.call_method(
                        &dev,
                        jni_str!("createRfcommSocketToServiceRecord"),
                        jni_sig!("(Ljava/util/UUID;)Landroid/bluetooth/BluetoothSocket;"),
                        &[JValue::Object(&uuid)],
                    ),
                    1 => env.call_method(
                        &dev,
                        jni_str!("createInsecureRfcommSocketToServiceRecord"),
                        jni_sig!("(Ljava/util/UUID;)Landroid/bluetooth/BluetoothSocket;"),
                        &[JValue::Object(&uuid)],
                    ),
                    // API tersembunyi; bila diblokir Android, gagal di sini dan error terakhir dilaporkan.
                    _ => env.call_method(&dev, jni_str!("createRfcommSocket"), jni_sig!("(I)Landroid/bluetooth/BluetoothSocket;"), &[JValue::Int(1)]),
                }
                .and_then(|v| v.l());
                let sock = match sock {
                    Ok(s) => s,
                    Err(e) => {
                        env.exception_clear();
                        last = Err(e);
                        continue;
                    }
                };
                let sent = (|| -> jni::errors::Result<()> {
                    env.call_method(&sock, jni_str!("connect"), jni_sig!("()V"), &[])?;
                    let os = env.call_method(&sock, jni_str!("getOutputStream"), jni_sig!("()Ljava/io/OutputStream;"), &[])?.l()?;
                    let bytes = env.byte_array_from_slice(data)?;
                    env.call_method(&os, jni_str!("write"), jni_sig!("([B)V"), &[JValue::Object(&bytes)])?;
                    env.call_method(&os, jni_str!("flush"), jni_sig!("()V"), &[])?;
                    Ok(())
                })();
                // Selalu tutup soket, juga saat gagal, supaya printer tidak "terkunci" ke HP ini.
                env.exception_clear();
                let _ = env.call_method(&sock, jni_str!("close"), jni_sig!("()V"), &[]);
                env.exception_clear();
                match sent {
                    Ok(()) => return Ok(true),
                    Err(e) => last = Err(e),
                }
            }
            last.map(|_| true)
        })?;
        if res {
            Ok(())
        } else {
            Err("Izinkan akses Perangkat sekitar (Bluetooth), lalu coba lagi".into())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bt_addr_vs_network() {
        assert!(super::is_bt_addr("00:11:22:AA:bb:CC"));
        assert!(!super::is_bt_addr("192.168.1.50:9100"));
        assert!(!super::is_bt_addr("192.168.1.50"));
        assert!(!super::is_bt_addr("00:11:22:AA:BB"));
    }
}
