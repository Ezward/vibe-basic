#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

# Parse arguments
BUILD_ALL=false
for arg in "$@"; do
    case "$arg" in
        --all) BUILD_ALL=true ;;
        *)
            echo "Usage: $0 [--all]"
            echo "  --all  Build native target plus all cross-compilation targets"
            echo "  (default: build native target only)"
            exit 1
            ;;
    esac
done

# --- Ensure cross-compilation tools are set up ---
if [ "$BUILD_ALL" = true ]; then
    echo "=== Running setup ==="
    "$SCRIPT_DIR/setup-cross-compile.sh"
fi

# Detect native target and OS
NATIVE_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"
OS="$(uname -s)"
case "$OS" in
    MINGW*|MSYS*|CYGWIN*) OS="Windows" ;;
esac

# Build target list
TARGETS=("$NATIVE_TARGET")

if [ "$BUILD_ALL" = true ]; then
    if [ "$OS" = "Darwin" ]; then
        TARGETS+=(
            x86_64-unknown-linux-gnu
            x86_64-unknown-linux-musl
            x86_64-pc-windows-gnu
            aarch64-pc-windows-gnullvm
        )
    elif [ "$OS" = "Linux" ]; then
        TARGETS+=(
            x86_64-pc-windows-gnu
            aarch64-pc-windows-gnullvm
        )
    fi
    # Windows: no cross-compile targets, native only
fi

FAILED=()

echo ""
if [ "$BUILD_ALL" = true ]; then
    echo "=== Building all targets ==="
else
    echo "=== Building native target ==="
fi
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
