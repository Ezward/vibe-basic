#!/usr/bin/env bash
set -euo pipefail

# Setup script for cross-compiling this Rust project from Mac to Linux and Windows.
# This script is idempotent — safe to run multiple times.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

CARGO_CONFIG=".cargo/config.toml"

echo "=== Cross-compilation setup ==="

# --- Rustup targets ---

add_rustup_target() {
    local target="$1"
    if rustup target list --installed | grep -q "^${target}$"; then
        echo "[skip] rustup target '${target}' already installed"
    else
        echo "[install] Adding rustup target '${target}'..."
        rustup target add "$target"
    fi
}

echo ""
echo "--- Rustup targets ---"
add_rustup_target x86_64-unknown-linux-gnu
add_rustup_target x86_64-unknown-linux-musl
add_rustup_target x86_64-pc-windows-gnu

# --- Homebrew taps and packages ---

add_brew_tap() {
    local tap="$1"
    if brew tap | grep -q "^${tap}$"; then
        echo "[skip] brew tap '${tap}' already tapped"
    else
        echo "[install] Tapping '${tap}'..."
        brew tap "$tap"
    fi
}

install_brew_package() {
    local package="$1"
    local display_name="${2:-$1}"
    if brew list "$package" &>/dev/null; then
        echo "[skip] brew package '${display_name}' already installed"
    else
        echo "[install] Installing '${display_name}' (this may take a while)..."
        brew install "$package"
    fi
}

echo ""
echo "--- Homebrew packages ---"

# Linux GNU cross-compiler
add_brew_tap messense/macos-cross-toolchains
install_brew_package x86_64-unknown-linux-gnu x86_64-unknown-linux-gnu

# Linux musl cross-compiler
add_brew_tap FiloSottile/musl-cross
install_brew_package FiloSottile/musl-cross/musl-cross musl-cross

# Windows MinGW cross-compiler
install_brew_package mingw-w64 mingw-w64

# --- Cargo config ---

echo ""
echo "--- Cargo config (${CARGO_CONFIG}) ---"

mkdir -p .cargo

ensure_cargo_config_section() {
    local header="$1"
    local body="$2"

    if [ ! -f "$CARGO_CONFIG" ]; then
        touch "$CARGO_CONFIG"
    fi

    if grep -qF "$header" "$CARGO_CONFIG"; then
        echo "[skip] Section '${header}' already in ${CARGO_CONFIG}"
    else
        echo "[add] Adding '${header}' to ${CARGO_CONFIG}"
        # Add a blank line before the section if the file is non-empty
        if [ -s "$CARGO_CONFIG" ]; then
            printf '\n' >> "$CARGO_CONFIG"
        fi
        printf '%s\n%s\n' "$header" "$body" >> "$CARGO_CONFIG"
    fi
}

ensure_cargo_config_section \
    '[target.x86_64-unknown-linux-gnu]' \
    'linker = "x86_64-unknown-linux-gnu-gcc"'

ensure_cargo_config_section \
    '[target.x86_64-unknown-linux-musl]' \
    'linker = "x86_64-linux-musl-gcc"'

ensure_cargo_config_section \
    '[target.x86_64-pc-windows-gnu]' \
    'linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"'

echo ""
echo "=== Setup complete ==="
echo ""
echo "You can now cross-compile with:"
echo "  cargo build --release --target x86_64-unknown-linux-gnu"
echo "  cargo build --release --target x86_64-unknown-linux-musl"
echo "  cargo build --release --target x86_64-pc-windows-gnu"
