#!/usr/bin/env bash
# Builds arm64-v8a + x86_64 shared libraries with cargo-ndk, combines them with
# the generated Kotlin bindings, and assembles target/android/OfflineCore.aar,
# which samples/android consumes.
#
# Requires Android NDK 27+ (ANDROID_NDK_HOME), Gradle 8.9+, and:
#   cargo install cargo-ndk --locked
#   rustup target add aarch64-linux-android x86_64-linux-android
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_ROOT="$ROOT/target/android"
PROJECT_ROOT="$ARTIFACT_ROOT/project"
SDK_ROOT="$PROJECT_ROOT/sdk"
GENERATED="$ARTIFACT_ROOT/generated"

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "cargo-ndk is required; install it with: cargo install cargo-ndk --locked" >&2
  exit 1
fi
if ! command -v gradle >/dev/null 2>&1; then
  echo "Gradle is required to assemble the AAR (brew install gradle)." >&2
  exit 1
fi

cd "$ROOT"

case "$(uname -s)" in
  Darwin) HOST_LIBRARY="target/release/liboffline_core.dylib" ;;
  *) HOST_LIBRARY="target/release/liboffline_core.so" ;;
esac

# Host build: only used to read UniFFI metadata for binding generation.
cargo build --release --package offline-core
cargo run --release --package uniffi-bindgen -- generate \
  --library "$HOST_LIBRARY" \
  --language kotlin \
  --out-dir "$GENERATED" \
  --no-format

rm -rf "$PROJECT_ROOT"
mkdir -p \
  "$SDK_ROOT/src/main/java/uniffi/offline_core" \
  "$SDK_ROOT/src/main/jniLibs"

cargo ndk \
  --platform 26 \
  -t arm64-v8a \
  -t x86_64 \
  -o "$SDK_ROOT/src/main/jniLibs" \
  build --release --package offline-core

cp "$GENERATED/uniffi/offline_core/offline_core.kt" \
  "$SDK_ROOT/src/main/java/uniffi/offline_core/"
cp packaging/android/settings.gradle.kts "$PROJECT_ROOT/settings.gradle.kts"
cp packaging/android/build.gradle.kts "$PROJECT_ROOT/build.gradle.kts"
cp packaging/android/sdk.build.gradle.kts "$SDK_ROOT/build.gradle.kts"
cp packaging/android/AndroidManifest.xml "$SDK_ROOT/src/main/AndroidManifest.xml"
cp packaging/android/consumer-rules.pro "$SDK_ROOT/consumer-rules.pro"

gradle -p "$PROJECT_ROOT" :sdk:assembleRelease
cp "$SDK_ROOT/build/outputs/aar/sdk-release.aar" "$ARTIFACT_ROOT/OfflineCore.aar"

echo "AAR ready: $ARTIFACT_ROOT/OfflineCore.aar"
