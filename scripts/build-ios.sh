#!/usr/bin/env bash
# Builds OfflineCore.xcframework (device + Apple Silicon simulator) and wraps it
# with the generated Swift bindings in a local Swift package that
# samples/ios consumes.
#
# Requires macOS with Xcode 16+ and:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_ROOT="$ROOT/target/ios"
PACKAGE_ROOT="$ARTIFACT_ROOT/OfflineCorePackage"
HEADERS_ROOT="$ARTIFACT_ROOT/headers"
GENERATED="$ARTIFACT_ROOT/generated"
XCFRAMEWORK="$PACKAGE_ROOT/OfflineCore.xcframework"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "build-ios.sh needs macOS + Xcode (xcodebuild, xcrun)." >&2
  exit 1
fi

cd "$ROOT"

# Host build: only used to read UniFFI metadata for binding generation.
cargo build --release --package offline-core
cargo run --release --package uniffi-bindgen -- generate \
  --library target/release/liboffline_core.dylib \
  --language swift \
  --out-dir "$GENERATED"

cargo build --release --package offline-core --target aarch64-apple-ios
cargo build --release --package offline-core --target aarch64-apple-ios-sim

rm -rf "$HEADERS_ROOT" "$PACKAGE_ROOT"
mkdir -p "$HEADERS_ROOT" "$PACKAGE_ROOT/Sources/OfflineCore"

cp "$GENERATED/offline_coreFFI.h" "$HEADERS_ROOT/"
cp "$GENERATED/offline_coreFFI.modulemap" "$HEADERS_ROOT/module.modulemap"
cp "$GENERATED/offline_core.swift" "$PACKAGE_ROOT/Sources/OfflineCore/"
cp packaging/ios/Package.swift "$PACKAGE_ROOT/Package.swift"

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/liboffline_core.a \
  -headers "$HEADERS_ROOT" \
  -library target/aarch64-apple-ios-sim/release/liboffline_core.a \
  -headers "$HEADERS_ROOT" \
  -output "$XCFRAMEWORK"

# Prove the package compiles for a real device before handing it to Xcode.
swift build \
  --package-path "$PACKAGE_ROOT" \
  --triple arm64-apple-ios17.0 \
  --sdk "$(xcrun --sdk iphoneos --show-sdk-path)"

echo "Swift package ready: $PACKAGE_ROOT"
