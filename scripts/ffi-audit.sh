#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if cargo tree --workspace --edges normal 2>/dev/null | \
   grep -E '(freetype-sys|fontconfig-sys|harfbuzz-sys| brotli v|flate2 v|miniz_oxide v|ring v)'; then
    echo "FFI LEAK DETECTED in oxifont"
    exit 1
fi
echo "oxifont FFI audit: CLEAN"
