#!/usr/bin/env bash
# ──────────────────────────────────────────────────
# S-ION Guest Agent Build Script
# ──────────────────────────────────────────────────
#
# Builds the Guest Agent as a static musl binary for injection
# into Firecracker MicroVMs, Apple vz, or WSL2 sidecars.
#
# Target: x86_64-unknown-linux-musl (static, < 2MB)
# Output: dist/sion-guest-agent
#
# Prerequisites:
#   - Docker (for `cross` cross-compilation from macOS/Windows)
#   - OR: `rustup target add x86_64-unknown-linux-musl` (Linux only)
#
# Usage:
#   ./build_guest.sh           # Build using `cross` (default, works on any OS)
#   ./build_guest.sh --native  # Build natively (Linux only, no Docker needed)
#   ./build_guest.sh --dev     # Build debug binary for local testing (macOS/Linux)
# ──────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUEST_DIR="${SCRIPT_DIR}/guest-agent"
DIST_DIR="${SCRIPT_DIR}/dist"
TARGET="x86_64-unknown-linux-musl"
BINARY_NAME="sion-guest-agent"

echo "╔══════════════════════════════════════════════════╗"
echo "║  🏗️  S-ION Guest Agent Builder                   ║"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# Ensure guest-agent directory exists
if [ ! -d "${GUEST_DIR}" ]; then
    echo "❌ guest-agent/ directory not found at ${GUEST_DIR}"
    exit 1
fi

# Create dist directory
mkdir -p "${DIST_DIR}"

# Parse arguments
BUILD_MODE="${1:---cross}"

case "${BUILD_MODE}" in
    --native)
        # ── Native Build (Linux Only) ──
        echo "🔧 Mode: Native musl build (Linux only)"
        echo "   Target: ${TARGET}"
        echo ""

        # Check if we're on Linux
        if [[ "$(uname -s)" != "Linux" ]]; then
            echo "❌ Native musl build requires Linux."
            echo "   Use './build_guest.sh' (default) for cross-compilation via Docker."
            exit 1
        fi

        # Ensure musl target is installed
        if ! rustup target list --installed | grep -q "${TARGET}"; then
            echo "📦 Installing musl target..."
            rustup target add "${TARGET}"
        fi

        # Ensure musl-tools is available
        if ! command -v musl-gcc &> /dev/null; then
            echo "📦 Installing musl-tools..."
            sudo apt-get update && sudo apt-get install -y musl-tools
        fi

        echo "🔨 Building..."
        cd "${GUEST_DIR}"
        cargo build --target "${TARGET}" --release

        # Copy to dist
        cp "target/${TARGET}/release/${BINARY_NAME}" "${DIST_DIR}/${BINARY_NAME}"
        ;;

    --dev)
        # ── Dev Build (Local OS, debug mode) ──
        echo "🔧 Mode: Local development build (debug)"
        echo "   Target: native ($(uname -m))"
        echo ""

        cd "${GUEST_DIR}"
        cargo build

        # Copy to dist — use the native debug binary
        cp "target/debug/${BINARY_NAME}" "${DIST_DIR}/${BINARY_NAME}"
        ;;

    --cross|*)
        # ── Cross Build (Any OS → Linux musl via Docker) ──
        echo "🔧 Mode: Cross-compilation via Docker"
        echo "   Target: ${TARGET}"
        echo ""

        # Check Docker is running
        if ! docker info &> /dev/null 2>&1; then
            echo "❌ Docker is not running."
            echo "   Cross-compilation requires Docker for musl builds on macOS/Windows."
            echo ""
            echo "   Options:"
            echo "   1. Start Docker Desktop and re-run this script"
            echo "   2. Use './build_guest.sh --dev' for local dev/testing builds"
            exit 1
        fi

        # Install cross if not present
        if ! command -v cross &> /dev/null; then
            echo "📦 Installing 'cross' (Rust cross-compilation tool)..."
            cargo install cross --git https://github.com/cross-rs/cross
        fi

        echo "🔨 Building with cross..."
        cd "${GUEST_DIR}"
        cross build --target "${TARGET}" --release

        # Copy to dist
        cp "target/${TARGET}/release/${BINARY_NAME}" "${DIST_DIR}/${BINARY_NAME}"
        ;;
esac

# Report
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
BINARY_PATH="${DIST_DIR}/${BINARY_NAME}"
if [ -f "${BINARY_PATH}" ]; then
    SIZE=$(du -h "${BINARY_PATH}" | cut -f1)
    echo "✅ Build complete!"
    echo "   Binary: ${BINARY_PATH}"
    echo "   Size:   ${SIZE}"

    # Check if it's a static binary (Linux only)
    if command -v file &> /dev/null; then
        FILE_TYPE=$(file "${BINARY_PATH}")
        echo "   Type:   ${FILE_TYPE}"

        if echo "${FILE_TYPE}" | grep -q "statically linked"; then
            echo "   Static: ✅ Yes (zero dependencies)"
        elif echo "${FILE_TYPE}" | grep -q "Mach-O"; then
            echo "   Static: ⚠️  macOS dev build (use --cross for production)"
        fi
    fi
else
    echo "❌ Build failed — binary not found at ${BINARY_PATH}"
    exit 1
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
