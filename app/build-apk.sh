#!/bin/sh
# Membuat APK Android Bakadai Lite. Semua alat ada di app/.tools (tidak mengubah sistem).
# Pemakaian: sh build-apk.sh   → hasil: target/release/apk/BakadaiLite.apk (pasang langsung)
#                                 dan target/release/apk/BakadaiLite.aab (unggah ke Play Store)
set -e
cd "$(dirname "$0")"
APP="$(pwd)"
export RUSTUP_HOME="$APP/.tools/rustup"
export CARGO_HOME="$APP/.tools/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
if [ -z "$ANDROID_NDK_ROOT" ]; then
  ANDROID_NDK_ROOT="$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)"
fi
export ANDROID_NDK_ROOT ANDROID_NDK_HOME="$ANDROID_NDK_ROOT"
if [ -z "$JAVA_HOME" ]; then
  for j in "$HOME/Library/Application Support/craft-anything/engines/jdk/Home" "$(/usr/libexec/java_home 2>/dev/null)"; do
    if [ -x "$j/bin/keytool" ]; then JAVA_HOME="$j"; break; fi
  done
fi
export JAVA_HOME PATH="$JAVA_HOME/bin:$PATH"

if [ ! -x "$CARGO_HOME/bin/cargo-apk" ]; then
  echo "Memasang alat (sekali saja)…"
  mkdir -p .tools
  curl -sSf https://sh.rustup.rs -o .tools/rustup-init.sh
  sh .tools/rustup-init.sh -y --profile minimal --default-toolchain stable --no-modify-path --target aarch64-linux-android
  cargo install cargo-apk --locked
fi

KS=.tools/bakadai-lite.keystore
PASS_FILE=.tools/bakadai-lite.keystore.pass
if [ ! -f "$KS" ]; then
  # Kunci baru = aplikasi baru di mata Android/Play. Kalau ini HP/komputer baru, salin kunci + sandi lama ke .tools dulu.
  (umask 077; openssl rand -base64 24 | tr -d '/+=' | cut -c1-28 | tr -d '\n' > "$PASS_FILE")
  keytool -genkeypair -v -keystore "$KS" -alias bakadailite -keyalg RSA -keysize 2048 -validity 10000 \
    -storepass "$(cat "$PASS_FILE")" -dname "CN=Bakadai Lite, O=Bakadai Lite, C=ID"
fi
export CARGO_APK_RELEASE_KEYSTORE="$APP/$KS"
CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$(cat "$PASS_FILE")"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD

cargo apk build --release --lib

# Android 16 + targetSdk 36: tombol/gesture kembali tidak lagi dikirim sebagai KEYCODE_BACK (predictive back) dan
# aplikasi langsung ditutup sistem. cargo-apk belum punya opsi manifest untuk ini, jadi manifest dikompilasi ulang
# dengan android:enableOnBackInvokedCallback="false" (opt-out resmi, lihat behavior-changes-16) lalu APK ditandatangani ulang.
APKDIR=target/release/apk
BT0="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
PLAT="$ANDROID_HOME/platforms/$(ls "$ANDROID_HOME/platforms" | sort -V | tail -1)/android.jar"
MW="$APKDIR/manifest-work"
rm -rf "$MW" && mkdir -p "$MW"
sed 's/<application /<application android:enableOnBackInvokedCallback="false" /' "$APKDIR/AndroidManifest.xml" > "$MW/AndroidManifest.xml"
grep -q 'enableOnBackInvokedCallback="false"' "$MW/AndroidManifest.xml"
"$BT0/aapt" package -f -M "$MW/AndroidManifest.xml" -S android/res -I "$PLAT" -F "$MW/manifest.apk"
(cd "$MW" && unzip -q -o manifest.apk AndroidManifest.xml)
cp "$APKDIR/BakadaiLite-unaligned.apk" "$MW/patched.apk"
zip -q -d "$MW/patched.apk" AndroidManifest.xml 'META-INF/*' >/dev/null 2>&1 || true
(cd "$MW" && zip -q patched.apk AndroidManifest.xml)
rm -f "$APKDIR/BakadaiLite.apk" "$APKDIR/BakadaiLite.apk.idsig"
"$BT0/zipalign" -p -f 4 "$MW/patched.apk" "$APKDIR/BakadaiLite.apk"
"$BT0/apksigner" sign --ks "$KS" --ks-pass env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD "$APKDIR/BakadaiLite.apk"
rm -rf "$MW"

# AAB untuk Play Store: ubah APK ke format proto, susun modul "base", lalu bundletool + tanda tangan upload key.
OUT=target/release/apk
BT="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
W="$OUT/aab-work"
rm -rf "$W" "$OUT/BakadaiLite.aab"
mkdir -p "$W/base"
"$BT/aapt2" convert --output-format proto -o "$W/proto.apk" "$OUT/BakadaiLite.apk"
(
  cd "$W/base"
  unzip -q ../proto.apk
  rm -rf META-INF
  mkdir manifest && mv AndroidManifest.xml manifest/
  if ls ./*.dex >/dev/null 2>&1; then mkdir dex && mv ./*.dex dex/; fi
  zip -qr ../base.zip .
)
java -jar .tools/bundletool-1.18.3.jar build-bundle --modules="$W/base.zip" --output="$OUT/BakadaiLite.aab"
jarsigner -keystore "$KS" -storepass:env CARGO_APK_RELEASE_KEYSTORE_PASSWORD -sigalg SHA256withRSA -digestalg SHA-256 \
  "$OUT/BakadaiLite.aab" bakadailite >/dev/null
rm -rf "$W"
ls -lh "$OUT"/*.apk "$OUT"/*.aab
