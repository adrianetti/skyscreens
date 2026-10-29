#!/bin/zsh
# Builds the headset APK without Gradle: cargo for the native library, then aapt2,
# zipalign and apksigner from the Android SDK. Pass --install to install it over adb.
set -euo pipefail
cd "$(dirname "$0")"

ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
NDK="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}"
BUILD_TOOLS="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-32/android.jar"
JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17}"
OPENXR_LOADER=1.1.65
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$JAVA_HOME/bin:$PATH"

OUT=../target/apk
mkdir -p "$OUT"

# Khronos OpenXR loader for Android, cached after the first build.
LOADER="$OUT/openxr/jni/arm64-v8a/libopenxr_loader.so"
if [[ ! -f "$LOADER" ]]; then
  curl -fsSL -o "$OUT/openxr.aar" "https://repo1.maven.org/maven2/org/khronos/openxr/openxr_loader_for_android/$OPENXR_LOADER/openxr_loader_for_android-$OPENXR_LOADER.aar"
  unzip -oq "$OUT/openxr.aar" "jni/arm64-v8a/*" -d "$OUT/openxr"
fi

TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/aarch64-linux-android29-clang"
export CC_aarch64_linux_android="$TOOLCHAIN/aarch64-linux-android29-clang"
export AR_aarch64_linux_android="$TOOLCHAIN/llvm-ar"
cargo build --release --target aarch64-linux-android -p xr

STAGE="$OUT/stage"
rm -rf "$STAGE" && mkdir -p "$STAGE/lib/arm64-v8a"
cp ../target/aarch64-linux-android/release/libskyscreens.so "$LOADER" "$STAGE/lib/arm64-v8a/"
"$BUILD_TOOLS/aapt2" link -o "$OUT/unsigned.apk" --manifest android/AndroidManifest.xml -I "$PLATFORM"
(cd "$STAGE" && zip -qr ../unsigned.apk lib)
"$BUILD_TOOLS/zipalign" -f -p 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"

KEYSTORE="$OUT/debug.keystore"
if [[ ! -f "$KEYSTORE" ]]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android -alias debug \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=skyscreens debug" > /dev/null 2>&1
fi
"$BUILD_TOOLS/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --out "$OUT/skyscreens.apk" "$OUT/aligned.apk"
ls -la "$OUT/skyscreens.apk" | awk '{print $5, $9}'

if [[ "${1:-}" == "--install" ]]; then
  "$ANDROID_HOME/platform-tools/adb" install -r "$OUT/skyscreens.apk"
fi
