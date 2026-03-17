#!/usr/bin/env bash
set -euo pipefail

# Setup script for cross-compiling this Rust project.
# Supports macOS (Homebrew) and Linux (apt, dnf/yum, pacman).
# This script is idempotent — safe to run multiple times.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

CARGO_CONFIG=".cargo/config.toml"
OS="$(uname -s)"

echo "=== Cross-compilation setup (${OS}) ==="

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

if [ "$OS" = "Darwin" ]; then
    add_rustup_target x86_64-unknown-linux-gnu
    add_rustup_target x86_64-unknown-linux-musl
fi
add_rustup_target x86_64-pc-windows-gnu

# --- Package installation ---

echo ""
if [ "$OS" = "Darwin" ]; then
    echo "--- Homebrew packages ---"

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

    # Linux GNU cross-compiler
    add_brew_tap messense/macos-cross-toolchains
    install_brew_package x86_64-unknown-linux-gnu x86_64-unknown-linux-gnu

    # Linux musl cross-compiler
    add_brew_tap FiloSottile/musl-cross
    install_brew_package FiloSottile/musl-cross/musl-cross musl-cross

    # Windows MinGW cross-compiler
    install_brew_package mingw-w64 mingw-w64

elif [ "$OS" = "Linux" ]; then
    # Detect Linux package manager
    if command -v apt-get &>/dev/null; then
        DISTRO_FAMILY="debian"
    elif command -v dnf &>/dev/null; then
        DISTRO_FAMILY="fedora"
    elif command -v yum &>/dev/null; then
        DISTRO_FAMILY="fedora"
    elif command -v pacman &>/dev/null; then
        DISTRO_FAMILY="arch"
    else
        echo "Unsupported Linux distribution: no apt-get, dnf, yum, or pacman found"
        exit 1
    fi

    echo "--- Linux packages (${DISTRO_FAMILY}) ---"

    if [ "$DISTRO_FAMILY" = "debian" ]; then
        install_linux_package() {
            local package="$1"
            if dpkg -s "$package" &>/dev/null; then
                echo "[skip] package '${package}' already installed"
            else
                if [ "${APT_UPDATED:-false}" = false ]; then
                    echo "[update] Updating apt package list..."
                    sudo apt-get update -qq
                    APT_UPDATED=true
                fi
                echo "[install] Installing '${package}'..."
                sudo apt-get install -y "$package"
            fi
        }

    elif [ "$DISTRO_FAMILY" = "fedora" ]; then
        # Use dnf if available, fall back to yum
        if command -v dnf &>/dev/null; then
            PKG_CMD="dnf"
        else
            PKG_CMD="yum"
        fi

        install_linux_package() {
            local package="$1"
            if rpm -q "$package" &>/dev/null; then
                echo "[skip] package '${package}' already installed"
            else
                echo "[install] Installing '${package}'..."
                sudo "$PKG_CMD" install -y "$package"
            fi
        }

    elif [ "$DISTRO_FAMILY" = "arch" ]; then
        install_linux_package() {
            local package="$1"
            if pacman -Qi "$package" &>/dev/null; then
                echo "[skip] package '${package}' already installed"
            else
                echo "[install] Installing '${package}'..."
                sudo pacman -S --noconfirm "$package"
            fi
        }
    fi

    # The mingw-w64 gcc package name varies by distro
    case "$DISTRO_FAMILY" in
        debian) install_linux_package mingw-w64 ;;
        fedora) install_linux_package mingw64-gcc ;;
        arch)   install_linux_package mingw-w64-gcc ;;
    esac

else
    echo "Unsupported OS: ${OS}"
    exit 1
fi

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

if [ "$OS" = "Darwin" ]; then
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
fi

if [ "$OS" = "Linux" ]; then
    ensure_cargo_config_section \
        '[target.x86_64-pc-windows-gnu]' \
        'linker = "x86_64-w64-mingw32-gcc"'
fi

echo ""
echo "=== Setup complete ==="
echo ""
echo "You can now cross-compile with:"
if [ "$OS" = "Darwin" ]; then
    echo "  cargo build --release --target x86_64-unknown-linux-gnu"
    echo "  cargo build --release --target x86_64-unknown-linux-musl"
fi
echo "  cargo build --release --target x86_64-pc-windows-gnu"
