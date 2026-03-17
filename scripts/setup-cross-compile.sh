#!/usr/bin/env bash
set -euo pipefail

# Setup script for cross-compiling this Rust project.
# Supports macOS (Homebrew), Linux (apt, dnf/yum, pacman), and Windows (MSYS2/Git Bash).
# This script is idempotent — safe to run multiple times.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

CARGO_CONFIG=".cargo/config.toml"
OS="$(uname -s)"

# Normalize Windows variants (MINGW64_NT-*, MSYS_NT-*, CYGWIN_NT-*) to "Windows"
case "$OS" in
    MINGW*|MSYS*|CYGWIN*) OS="Windows" ;;
esac

echo "=== Cross-compilation setup (${OS}) ==="

# --- llvm-mingw for ARM Windows cross-compilation ---

LLVM_MINGW_DIR="$HOME/.local/share/llvm-mingw"

install_llvm_mingw() {
    if [ -x "$LLVM_MINGW_DIR/bin/aarch64-w64-mingw32-clang" ]; then
        echo "[skip] llvm-mingw already installed at ${LLVM_MINGW_DIR}"
    else
        echo "[install] Downloading llvm-mingw (ARM Windows cross-compiler)..."
        local arch platform tag url tmpdir
        arch="$(uname -m)"

        # Get latest release tag from GitHub
        tag="$(curl -sL https://api.github.com/repos/mstorsjo/llvm-mingw/releases/latest | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
        if [ -z "$tag" ]; then
            echo "[error] Failed to determine latest llvm-mingw release"
            return 1
        fi

        if [ "$OS" = "Darwin" ]; then
            platform="macos-universal"
        else
            # Determine the Ubuntu version suffix used in llvm-mingw release asset names
            local ubuntu_ver
            ubuntu_ver="$(curl -fsSL "https://api.github.com/repos/mstorsjo/llvm-mingw/releases/tags/${tag}" \
                | grep -o "ucrt-ubuntu-[0-9.]*-${arch}" | head -1 | sed "s/ucrt-//;s/-${arch}//")"
            if [ -z "$ubuntu_ver" ]; then
                ubuntu_ver="ubuntu-22.04"
            fi
            platform="${ubuntu_ver}-${arch}"
        fi

        url="https://github.com/mstorsjo/llvm-mingw/releases/download/${tag}/llvm-mingw-${tag}-ucrt-${platform}.tar.xz"
        echo "[install] Downloading ${url}..."

        tmpdir="$(mktemp -d)"
        if ! curl -fSL "$url" -o "$tmpdir/llvm-mingw.tar.xz"; then
            echo "[error] Failed to download llvm-mingw from ${url}"
            rm -rf "$tmpdir"
            return 1
        fi
        if ! tar -xJf "$tmpdir/llvm-mingw.tar.xz" -C "$tmpdir"; then
            echo "[error] Failed to extract llvm-mingw"
            rm -rf "$tmpdir"
            return 1
        fi
        rm -f "$tmpdir/llvm-mingw.tar.xz"

        mkdir -p "$(dirname "$LLVM_MINGW_DIR")"
        rm -rf "$LLVM_MINGW_DIR"
        mv "$tmpdir"/llvm-mingw-* "$LLVM_MINGW_DIR"
        rm -rf "$tmpdir"
        echo "[ok] llvm-mingw installed to ${LLVM_MINGW_DIR}"
    fi
}

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
    add_rustup_target x86_64-pc-windows-gnu
    add_rustup_target aarch64-pc-windows-gnullvm
elif [ "$OS" = "Linux" ]; then
    add_rustup_target x86_64-pc-windows-gnu
    add_rustup_target aarch64-pc-windows-gnullvm
elif [ "$OS" = "Windows" ]; then
    # Ensure the native MSVC target is available (usually installed by default)
    add_rustup_target x86_64-pc-windows-msvc
    add_rustup_target aarch64-pc-windows-msvc
    echo "[info] No cross-compilation packages needed on Windows"
fi

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

    # Windows MinGW cross-compiler (x86_64)
    install_brew_package mingw-w64 mingw-w64

    # ARM Windows cross-compiler (llvm-mingw)
    install_llvm_mingw

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

    # The mingw-w64 gcc package name varies by distro (x86_64)
    case "$DISTRO_FAMILY" in
        debian) install_linux_package mingw-w64 ;;
        fedora) install_linux_package mingw64-gcc ;;
        arch)   install_linux_package mingw-w64-gcc ;;
    esac

    # ARM Windows cross-compiler (llvm-mingw)
    install_llvm_mingw

elif [ "$OS" = "Windows" ]; then
    echo "--- Windows packages ---"
    echo "[skip] No additional packages needed for native Windows builds"

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

    ensure_cargo_config_section \
        '[target.aarch64-pc-windows-gnullvm]' \
        "linker = \"${LLVM_MINGW_DIR}/bin/aarch64-w64-mingw32-clang\""
fi

if [ "$OS" = "Linux" ]; then
    ensure_cargo_config_section \
        '[target.x86_64-pc-windows-gnu]' \
        'linker = "x86_64-w64-mingw32-gcc"'

    ensure_cargo_config_section \
        '[target.aarch64-pc-windows-gnullvm]' \
        "linker = \"${LLVM_MINGW_DIR}/bin/aarch64-w64-mingw32-clang\""
fi

# Windows native builds need no special Cargo linker configuration

echo ""
echo "=== Setup complete ==="
echo ""
echo "You can now build with:"
echo "  cargo build --release"
if [ "$OS" = "Darwin" ]; then
    echo ""
    echo "Cross-compile targets:"
    echo "  cargo build --release --target x86_64-unknown-linux-gnu"
    echo "  cargo build --release --target x86_64-unknown-linux-musl"
    echo "  cargo build --release --target x86_64-pc-windows-gnu"
    echo "  cargo build --release --target aarch64-pc-windows-gnullvm"
elif [ "$OS" = "Linux" ]; then
    echo ""
    echo "Cross-compile targets:"
    echo "  cargo build --release --target x86_64-pc-windows-gnu"
    echo "  cargo build --release --target aarch64-pc-windows-gnullvm"
fi
