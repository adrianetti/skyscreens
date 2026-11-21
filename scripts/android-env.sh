#!/usr/bin/env bash
NDK_VERSION=27.2.12479018
BUILD_TOOLS_VERSION=34.0.0
ANDROID_PLATFORM=android-32
ANDROID_API=29

if [[ -z "${ANDROID_HOME:-}" ]]; then
  for candidate in "${ANDROID_SDK_ROOT:-}" /opt/homebrew/share/android-commandlinetools "$HOME/Library/Android/sdk" /usr/local/lib/android/sdk; do
    if [[ -n "$candidate" && -d "$candidate" ]]; then
      ANDROID_HOME="$candidate"
      break
    fi
  done
fi
export ANDROID_HOME="${ANDROID_HOME:?Android SDK not found; set ANDROID_HOME}"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "Android NDK $NDK_VERSION missing; install it, see docs/development.md" >&2
  return 1
fi
export BUILD_TOOLS="$ANDROID_HOME/build-tools/$BUILD_TOOLS_VERSION"
export ANDROID_JAR="$ANDROID_HOME/platforms/$ANDROID_PLATFORM/android.jar"
export ADB="$ANDROID_HOME/platform-tools/adb"

if [[ -z "${JAVA_HOME:-}" && -d /opt/homebrew/opt/openjdk@17 ]]; then
  export JAVA_HOME=/opt/homebrew/opt/openjdk@17
fi

case "$(uname -s)" in
  Darwin) NDK_HOST=darwin-x86_64 ;;
  *) NDK_HOST=linux-x86_64 ;;
esac
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android$ANDROID_API-clang"
export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android$ANDROID_API-clang"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:${JAVA_HOME:+$JAVA_HOME/bin:}$PATH"
