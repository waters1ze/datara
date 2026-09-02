#!/usr/bin/env bash
# =====================================================================
# Datara & Forgen Official Universal Unix Installer (Linux & macOS)
# =====================================================================
set -e

RESET="\033[0m"
BOLD="\033[1m"
CYAN="\033[36m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"

echo -e "${CYAN}${BOLD}"
echo "======================================================================="
echo "   ____        _                     "
echo "  |  _ \  __ _| |_ __ _ _ __ __ _   "
echo "  | | | |/ _\` | __/ _\` | '__/ _\` |   DATARA SYSTEMS LANGUAGE"
echo "  | |_| | (_| | || (_| | | | (_| |   Forgen AOT Native Toolchain v0.1.0"
echo "  |____/ \__,_|\__\__,_|_|  \__,_|   https://github.com/waters1ze/datara"
echo "======================================================================="
echo -e "${RESET}"

echo -e "-> Initializing Datara & Forgen Unix Installation..."

INSTALL_DIR="${HOME}/.datara"
BIN_DIR="${INSTALL_DIR}/bin"
STDLIB_DIR="${INSTALL_DIR}/stdlib"
RUNTIME_DIR="${INSTALL_DIR}/runtime"

mkdir -p "${BIN_DIR}"
mkdir -p "${STDLIB_DIR}"
mkdir -p "${RUNTIME_DIR}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd || echo "")"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." 2>/dev/null && pwd || echo "")"

OS_TYPE="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_TYPE="$(uname -m)"

echo -e "  [DETECT] Operating System : ${BOLD}${OS_TYPE}${RESET}"
echo -e "  [DETECT] Architecture     : ${BOLD}${ARCH_TYPE}${RESET}"

# 1. Locate compiler binary
SOURCE_BIN=""
if [ -f "${REPO_ROOT}/target/release/forgen" ]; then
    SOURCE_BIN="${REPO_ROOT}/target/release/forgen"
elif [ -f "${SCRIPT_DIR}/forgen" ]; then
    SOURCE_BIN="${SCRIPT_DIR}/forgen"
elif [ -f "./forgen" ]; then
    SOURCE_BIN="./forgen"
fi

if [ -n "${SOURCE_BIN}" ] && [ -f "${SOURCE_BIN}" ]; then
    cp -f "${SOURCE_BIN}" "${BIN_DIR}/forgen"
    chmod +x "${BIN_DIR}/forgen"
    echo -e "  [OK] Installed compiler binary: ${GREEN}${BIN_DIR}/forgen${RESET}"
else
    echo -e "  [INFO] Local forgen binary not found. Compiling via cargo..."
    if command -v cargo >/dev/null 2>&1; then
        cargo build --release
        cp -f "./target/release/forgen" "${BIN_DIR}/forgen"
        chmod +x "${BIN_DIR}/forgen"
        echo -e "  [OK] Compiled and installed: ${GREEN}${BIN_DIR}/forgen${RESET}"
    else
        echo -e "  ${RED}[ERROR] Neither pre-built forgen binary nor cargo was found.${RESET}"
        echo -e "  Please download the official release tarball or install Rust: https://rustup.rs"
        exit 1
    fi
fi

# 2. Install Standard Library
if [ -d "${REPO_ROOT}/stdlib" ]; then
    cp -rf "${REPO_ROOT}/stdlib/"* "${STDLIB_DIR}/"
    echo -e "  [OK] Installed Standard Library: ${GREEN}${STDLIB_DIR}${RESET}"
elif [ -d "${SCRIPT_DIR}/stdlib" ]; then
    cp -rf "${SCRIPT_DIR}/stdlib/"* "${STDLIB_DIR}/"
    echo -e "  [OK] Installed Standard Library: ${GREEN}${STDLIB_DIR}${RESET}"
fi

# 2b. Install the Native Runtime Library (required for linking)
RUNTIME_LIB_NAME="libdatara_runtime.a"
RUNTIME_SOURCE=""
for candidate in     "${REPO_ROOT}/runtime/${RUNTIME_LIB_NAME}"     "${SCRIPT_DIR}/${RUNTIME_LIB_NAME}"     "$(find "${REPO_ROOT}/target" -name "${RUNTIME_LIB_NAME}" 2>/dev/null | head -n 1)"; do
    if [ -n "$candidate" ] && [ -f "$candidate" ]; then
        RUNTIME_SOURCE="$candidate"
        break
    fi
done

if [ -n "${RUNTIME_SOURCE}" ]; then
    cp -f "${RUNTIME_SOURCE}" "${RUNTIME_DIR}/${RUNTIME_LIB_NAME}"
    echo -e "  [OK] Installed Native Runtime: ${GREEN}${RUNTIME_DIR}/${RUNTIME_LIB_NAME}${RESET}"
else
    echo -e "  ${YELLOW}[WARN] ${RUNTIME_LIB_NAME} not found next to the installer.${RESET}"
    echo -e "         Run 'cargo build --release' first; the runtime is compiled"
    echo -e "         into target/ and copied here. Without it, linking fails."
fi

# 3. Configure Shell Environment
PROFILE_FILES=()
[ -f "${HOME}/.bashrc" ] && PROFILE_FILES+=("${HOME}/.bashrc")
[ -f "${HOME}/.zshrc" ] && PROFILE_FILES+=("${HOME}/.zshrc")
[ -f "${HOME}/.profile" ] && PROFILE_FILES+=("${HOME}/.profile")

EXPORT_LINE="export PATH=\"\$HOME/.datara/bin:\$PATH\""
HOME_LINE="export DATARA_HOME=\"\$HOME/.datara\""

for p in "${PROFILE_FILES[@]}"; do
    if ! grep -q ".datara/bin" "$p" 2>/dev/null; then
        echo "" >> "$p"
        echo "# Datara Toolchain" >> "$p"
        echo "$EXPORT_LINE" >> "$p"
        echo "$HOME_LINE" >> "$p"
        echo -e "  [OK] Added PATH to ${GREEN}$p${RESET}"
    else
        echo -e "  [OK] PATH already configured in $p"
    fi
done

# 4. Check for system C compiler & Linker
if command -v gcc >/dev/null 2>&1 || command -v clang >/dev/null 2>&1; then
    echo -e "  [OK] Native C compiler detected."
else
    echo -e "  ${YELLOW}[INFO] C compiler (gcc / clang) not detected.${RESET}"
    echo -e "         To link native executables, install build essentials:"
    echo -e "           - Ubuntu/Debian : sudo apt install build-essential"
    echo -e "           - Fedora/RHEL   : sudo dnf groupinstall \"Development Tools\""
    echo -e "           - Arch Linux    : sudo pacman -S base-devel"
    echo -e "           - Alpine Linux  : sudo apk add build-base"
    echo -e "           - macOS         : xcode-select --install"
fi

# 5. Verification Test
echo ""
echo -e "${CYAN}-> Verifying Datara installation...${RESET}"
if "${BIN_DIR}/forgen" --help >/dev/null 2>&1; then
    echo -e "  [OK] Forgen CLI responds."
else
    echo -e "  ${RED}[ERROR] Installed forgen binary failed to run.${RESET}"
    exit 1
fi
if [ -f "${RUNTIME_DIR}/libdatara_runtime.a" ]; then
    echo -e "  ${GREEN}${BOLD}[SUCCESS] Datara installation verified (CLI + native runtime).${RESET}"
else
    echo -e "  ${YELLOW}[WARN] Native runtime library missing: linking will fail until it is installed.${RESET}"
fi

echo -e "${CYAN}"
echo "======================================================================="
echo "   DATARA INSTALLATION COMPLETE!"
echo "======================================================================="
echo -e "${RESET}"
echo "To activate Datara in your current shell session, run:"
echo -e "  ${BOLD}export PATH=\"\$HOME/.datara/bin:\$PATH\"${RESET}"
echo -e "  ${BOLD}export DATARA_HOME=\"\$HOME/.datara\"${RESET}"
echo ""
echo "Or restart your terminal window, then type:"
echo -e "  ${GREEN}forgen --help${RESET}"
echo ""
echo "Create your first project:"
echo -e "  ${GREEN}forgen new my_app && cd my_app && forgen run${RESET}"
echo "======================================================================="
