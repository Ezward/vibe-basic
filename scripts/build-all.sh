#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

# --- Ensure cross-compilation tools are set up ---
echo "=== Running setup ==="
"$SCRIPT_DIR/setup-cross-compile.sh"

# Detect native Mac target
NATIVE_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"

TARGETS=(
    "$NATIVE_TARGET"
    x86_64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    x86_64-pc-windows-gnu
)

FAILED=()

echo ""
echo "=== Building all targets ==="
for target in "${TARGETS[@]}"; do
    echo ""
    echo "--- Building: ${target} ---"
    if cargo build --release --target "$target"; then
        echo "[ok] ${target}"
    else
        echo "[FAILED] ${target}"
        FAILED+=("$target")
    fi
done

echo ""
echo "=== Build summary ==="
for target in "${TARGETS[@]}"; do
    if [ ${#FAILED[@]} -gt 0 ] && printf '%s\n' "${FAILED[@]}" | grep -qx "$target"; then
        echo "  FAILED  ${target}"
    else
        echo "  OK      ${target}"
    fi
done

if [ ${#FAILED[@]} -gt 0 ]; then
    echo ""
    echo "${#FAILED[@]} target(s) failed."
    exit 1
else
    echo ""
    echo "All targets built successfully."
    echo "Binaries are in target/<target>/release/"
fi
