#!/usr/bin/env bash
# Generates Swift, Kotlin and Python bindings from the compiled host library.
# Output goes to target/bindings/ (git-ignored); generated code is never
# committed. Pass --smoke to also run scripts/ffi_smoke.py against the Python
# bindings, which exercises the real C ABI on any host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/bindings"
cd "$ROOT"

cargo build --package offline-core

case "$(uname -s)" in
  Darwin) LIBRARY="target/debug/liboffline_core.dylib" ;;
  *) LIBRARY="target/debug/liboffline_core.so" ;;
esac

rm -rf "$OUT"
for language in swift kotlin python; do
  cargo run --quiet --package uniffi-bindgen -- generate \
    --library "$LIBRARY" \
    --language "$language" \
    --out-dir "$OUT/$language" \
    --no-format
done

echo "bindings written to $OUT"
find "$OUT" -type f | sort

if [[ "${1:-}" == "--smoke" ]]; then
  cp "$LIBRARY" "$OUT/python/"
  python3 "$ROOT/scripts/ffi_smoke.py" "$OUT/python"
fi
