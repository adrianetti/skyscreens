#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source ../scripts/android-env.sh

OPENXR_LOADER=1.1.65
OPENXR_LOADER_SHA256=a298c326085b09181c85235fb04d37510939bd30938ed4322c093c09780e8643
OUT=../target/apk
mkdir -p "$OUT"

LOADER="$OUT/openxr/jni/arm64-v8a/libopenxr_loader.so"
if [[ ! -f "$LOADER" ]]; then
  curl -fsSL -o "$OUT/openxr.aar" "https://repo1.maven.org/maven2/org/khronos/openxr/openxr_loader_for_android/$OPENXR_LOADER/openxr_loader_for_android-$OPENXR_LOADER.aar"
  echo "$OPENXR_LOADER_SHA256  $OUT/openxr.aar" | shasum -a 256 -c --quiet || { rm -f "$OUT/openxr.aar"; exit 1; }
  unzip -oq "$OUT/openxr.aar" "jni/arm64-v8a/*" -d "$OUT/openxr"
fi

cargo build --release --target aarch64-linux-android -p xr

STAGE="$OUT/stage"
rm -rf "$STAGE" && mkdir -p "$STAGE/lib/arm64-v8a"
cp ../target/aarch64-linux-android/release/libskyscreens.so "$LOADER" "$STAGE/lib/arm64-v8a/"
"$BUILD_TOOLS/aapt2" link -o "$OUT/unsigned.apk" --manifest android/AndroidManifest.xml -I "$ANDROID_JAR"
(cd "$STAGE" && zip -qr ../unsigned.apk lib)
"$BUILD_TOOLS/zipalign" -f -p 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"

KEYSTORE="$OUT/debug.keystore"
if [[ ! -f "$KEYSTORE" ]]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android -alias debug \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=skyscreens debug" > /dev/null 2>&1
fi
"$BUILD_TOOLS/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --out "$OUT/skyscreens.apk" "$OUT/aligned.apk"
wc -c < "$OUT/skyscreens.apk" | awk -v f="$OUT/skyscreens.apk" '{ print $1, f }'

if [[ "${1:-}" == "--install" ]]; then
  "$ADB" install -r "$OUT/skyscreens.apk"
fi
