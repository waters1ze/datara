#!/usr/bin/env bash
# Datara & Forgen Automated Universal Linux/macOS Installer
# Run: curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/install.sh | bash

set -euo pipefail

COLOR_CYAN='\033[0;36m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[1;33m'
COLOR_GRAY='\033[0;90m'
COLOR_NC='\033[0m'

echo -e "${COLOR_CYAN}================================================================================${COLOR_NC}"
echo -e "${COLOR_CYAN}   ____        _                     ${COLOR_NC}"
echo -e "${COLOR_CYAN}  |  _ \\  __ _| |_ __ _ _ __ __ _    Datara Systems Language${COLOR_NC}"
echo -e "${COLOR_CYAN}  | | | |/ _\` | __/ _\` | '__/ _\` |   Forgen AOT Native Toolchain${COLOR_NC}"
echo -e "${COLOR_CYAN}  | |_| | (_| | || (_| | | | (_| |   Universal Unix Installer${COLOR_NC}"
echo -e "${COLOR_CYAN}  |____/ \\__,_|\\__\\__,_|_|  \\__,_|   https://github.com/waters1ze/datara${COLOR_NC}"
echo -e "${COLOR_CYAN}================================================================================${COLOR_NC}"

INSTALL_DIR="${HOME}/.datara"
BIN_DIR="${INSTALL_DIR}/bin"
STDLIB_DIR="${INSTALL_DIR}/stdlib"
ASSETS_DIR="${INSTALL_DIR}/assets"

echo -e "\n${COLOR_YELLOW}[1/5] Preparing installation directories...${COLOR_NC}"
mkdir -p "${BIN_DIR}" "${STDLIB_DIR}" "${ASSETS_DIR}"

# 2. Dynamic Version Detection
echo -e "${COLOR_YELLOW}[2/5] Resolving latest Datara version from GitHub...${COLOR_NC}"
REPO="waters1ze/datara"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
LATEST_TAG="v0.1.0"
DOWNLOAD_URL=""

OS_TYPE="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_TYPE="$(uname -m)"
case "${ARCH_TYPE}" in
    x86_64|amd64) TARGET_ARCH="x64" ;;
    aarch64|arm64) TARGET_ARCH="aarch64" ;;
    *) TARGET_ARCH="x64" ;;
esac

if command -v curl >/dev/null 2>&1; then
    RELEASE_JSON="$(curl -sSL -H "User-Agent: Datara-Installer" --max-time 5 "${API_URL}" 2>/dev/null || echo "")"
    if [ -n "${RELEASE_JSON}" ]; then
        TAG_NAME="$(echo "${RELEASE_JSON}" | grep -o '"tag_name": *"[^"]*"' | head -n1 | cut -d'"' -f4 || echo "")"
        if [ -n "${TAG_NAME}" ]; then
            LATEST_TAG="${TAG_NAME}"
            echo -e "${COLOR_GREEN}  -> Detected latest release: ${LATEST_TAG}${COLOR_NC}"
            # Match release asset
            ASSET_URL="$(echo "${RELEASE_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -i "${OS_TYPE}" | grep -i "${TARGET_ARCH}" | head -n1 | cut -d'"' -f4 || echo "")"
            if [ -n "${ASSET_URL}" ]; then
                DOWNLOAD_URL="${ASSET_URL}"
            fi
        fi
    fi
fi

if [ -z "${DOWNLOAD_URL}" ]; then
    echo -e "${COLOR_GRAY}  -> Using release profile: ${LATEST_TAG}${COLOR_NC}"
fi

# 3. Obtain Binaries
echo -e "${COLOR_YELLOW}[3/5] Installing compiler binaries and runtime...${COLOR_NC}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd || echo "")"
INSTALLED=0

# 3a. Check local repo candidates
if [ -n "${SCRIPT_DIR}" ]; then
    if [ -f "${SCRIPT_DIR}/target/release/forgen" ]; then
        cp "${SCRIPT_DIR}/target/release/forgen" "${BIN_DIR}/forgen"
        cp "${SCRIPT_DIR}/target/release/forgen" "${BIN_DIR}/datara"
        chmod +x "${BIN_DIR}/forgen" "${BIN_DIR}/datara"
        echo -e "${COLOR_GREEN}  -> Copied binaries from target/release/${COLOR_NC}"
        INSTALLED=1
    elif [ -f "${SCRIPT_DIR}/forgen" ]; then
        cp "${SCRIPT_DIR}/forgen" "${BIN_DIR}/forgen"
        cp "${SCRIPT_DIR}/forgen" "${BIN_DIR}/datara"
        chmod +x "${BIN_DIR}/forgen" "${BIN_DIR}/datara"
        echo -e "${COLOR_GREEN}  -> Copied binaries from local directory${COLOR_NC}"
        INSTALLED=1
    fi
fi

# 3b. Download prebuilt release package
if [ "${INSTALLED}" -eq 0 ] && [ -n "${DOWNLOAD_URL}" ]; then
    echo -e "${COLOR_CYAN}  -> Downloading prebuilt package from GitHub Releases...${COLOR_NC}"
    TMP_PKG="$(mktemp -d)"
    if curl -sSL --max-time 60 "${DOWNLOAD_URL}" -o "${TMP_PKG}/datara.tar.gz" 2>/dev/null; then
        tar -xzf "${TMP_PKG}/datara.tar.gz" -C "${TMP_PKG}" 2>/dev/null || true
        EXTRACTED_BIN="$(find "${TMP_PKG}" -name forgen -type f | head -n1)"
        if [ -n "${EXTRACTED_BIN}" ]; then
            cp "${EXTRACTED_BIN}" "${BIN_DIR}/forgen"
            cp "${EXTRACTED_BIN}" "${BIN_DIR}/datara"
            chmod +x "${BIN_DIR}/forgen" "${BIN_DIR}/datara"
            echo -e "${COLOR_GREEN}  -> Successfully installed downloaded ${LATEST_TAG} binaries.${COLOR_NC}"
            INSTALLED=1
        fi
    fi
    rm -rf "${TMP_PKG}"
fi

# 3c. Fallback to Cargo if available
if [ "${INSTALLED}" -eq 0 ]; then
    CARGO_BIN="$(command -v cargo || echo "${HOME}/.cargo/bin/cargo")"
    if [ -x "${CARGO_BIN}" ] && [ -n "${SCRIPT_DIR}" ] && [ -f "${SCRIPT_DIR}/Cargo.toml" ]; then
        echo -e "${COLOR_CYAN}  -> Compiling toolchain via Cargo in release mode...${COLOR_NC}"
        (cd "${SCRIPT_DIR}" && "${CARGO_BIN}" build --release --bin forgen)
        cp "${SCRIPT_DIR}/target/release/forgen" "${BIN_DIR}/forgen"
        cp "${SCRIPT_DIR}/target/release/forgen" "${BIN_DIR}/datara"
        chmod +x "${BIN_DIR}/forgen" "${BIN_DIR}/datara"
        echo -e "${COLOR_GREEN}  -> Compilation finished and binaries installed.${COLOR_NC}"
        INSTALLED=1
    fi
fi

if [ "${INSTALLED}" -eq 0 ]; then
    echo "Failed to find or build forgen binary. Please clone https://github.com/waters1ze/datara and run cargo build --release."
    exit 1
fi

# 4. Install Standard Library & Assets
echo -e "${COLOR_YELLOW}[4/5] Installing standard library and icons...${COLOR_NC}"
if [ -n "${SCRIPT_DIR}" ] && [ -d "${SCRIPT_DIR}/stdlib" ]; then
    cp -r "${SCRIPT_DIR}/stdlib/"* "${STDLIB_DIR}/"
    echo -e "${COLOR_GREEN}  -> Installed stdlib to ${STDLIB_DIR}${COLOR_NC}"
fi

if [ -n "${SCRIPT_DIR}" ] && [ -d "${SCRIPT_DIR}/assets" ]; then
    cp -r "${SCRIPT_DIR}/assets/"* "${ASSETS_DIR}/"
fi

# Install Desktop Icons and File Associations
if [ "$(uname -s)" = "Darwin" ]; then
    if [ -n "${SCRIPT_DIR}" ] && [ -f "${SCRIPT_DIR}/scripts/install_macos_icons.sh" ]; then
        bash "${SCRIPT_DIR}/scripts/install_macos_icons.sh" 2>/dev/null || true
    fi
elif [ "$(uname -s)" = "Linux" ]; then
    if [ -n "${SCRIPT_DIR}" ] && [ -f "${SCRIPT_DIR}/scripts/install_linux_icons.sh" ]; then
        bash "${SCRIPT_DIR}/scripts/install_linux_icons.sh" 2>/dev/null || true
    fi
fi

# 5. Configure Shell Profile PATH
echo -e "${COLOR_YELLOW}[5/5] Configuring environment PATH...${COLOR_NC}"
PROFILE_FILE=""
if [ -n "${ZSH_VERSION:-}" ] || [ -f "${HOME}/.zshrc" ]; then
    PROFILE_FILE="${HOME}/.zshrc"
elif [ -f "${HOME}/.bashrc" ]; then
    PROFILE_FILE="${HOME}/.bashrc"
elif [ -f "${HOME}/.profile" ]; then
    PROFILE_FILE="${HOME}/.profile"
fi

if [ -n "${PROFILE_FILE}" ]; then
    if ! grep -q "DATARA_HOME" "${PROFILE_FILE}" 2>/dev/null; then
        echo "" >> "${PROFILE_FILE}"
        echo '# Datara Environment' >> "${PROFILE_FILE}"
        echo 'export DATARA_HOME="$HOME/.datara"' >> "${PROFILE_FILE}"
        echo 'export PATH="$HOME/.datara/bin:$PATH"' >> "${PROFILE_FILE}"
        echo -e "${COLOR_GREEN}  -> Added Datara to ${PROFILE_FILE}${COLOR_NC}"
    else
        echo -e "${COLOR_GRAY}  -> Datara already configured in ${PROFILE_FILE}${COLOR_NC}"
    fi
fi

export PATH="${BIN_DIR}:${PATH}"
export DATARA_HOME="${INSTALL_DIR}"

echo -e "\n${COLOR_GREEN}================================================================================${COLOR_NC}"
echo -e "${COLOR_GREEN} Verification & Environment Check:${COLOR_NC}"
echo -e "${COLOR_GREEN}================================================================================${COLOR_NC}"
if [ -x "${BIN_DIR}/forgen" ]; then
    "${BIN_DIR}/forgen" --version || true
fi
echo -e "DATARA_HOME: ${INSTALL_DIR}"
echo -e "\n${COLOR_CYAN}🎉 Datara & Forgen installed successfully!${COLOR_NC}"
echo -e "To start using it immediately, run:"
if [ -n "${PROFILE_FILE}" ]; then
    echo -e "  source ${PROFILE_FILE}"
fi
echo -e "  forgen repl"
echo -e "  forgen run main.dtr\n"
