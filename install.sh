#!/usr/bin/env bash
# Datara & Forgen Automated Linux/macOS Installer
# Run: curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/install.sh | bash

set -e

COLOR_CYAN='\033[0;36m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[1;33m'
COLOR_NC='\033[0m' # No Color

echo -e "${COLOR_CYAN}================================================================================${COLOR_NC}"
echo -e "${COLOR_CYAN} Datara Programming Language & Forgen Compiler — Unix Installer${COLOR_NC}"
echo -e "${COLOR_CYAN}================================================================================${COLOR_NC}"

INSTALL_DIR="$HOME/.datara"
BIN_DIR="$INSTALL_DIR/bin"
STDLIB_DIR="$INSTALL_DIR/stdlib"

echo -e "\n${COLOR_YELLOW}[1/4] Preparing directories...${COLOR_NC}"
mkdir -p "$BIN_DIR"
mkdir -p "$STDLIB_DIR"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${COLOR_YELLOW}[2/4] Installing compiler binaries...${COLOR_NC}"
if [ -f "$SCRIPT_DIR/target/release/forgen" ]; then
    cp "$SCRIPT_DIR/target/release/forgen" "$BIN_DIR/forgen"
    cp "$SCRIPT_DIR/target/release/forgen" "$BIN_DIR/datara"
    chmod +x "$BIN_DIR/forgen" "$BIN_DIR/datara"
    echo -e "${COLOR_GREEN}  -> Copied binaries from target/release/${COLOR_NC}"
elif [ -f "$SCRIPT_DIR/forgen" ]; then
    cp "$SCRIPT_DIR/forgen" "$BIN_DIR/forgen"
    cp "$SCRIPT_DIR/forgen" "$BIN_DIR/datara"
    chmod +x "$BIN_DIR/forgen" "$BIN_DIR/datara"
    echo -e "${COLOR_GREEN}  -> Copied binaries from root${COLOR_NC}"
else
    echo -e "${COLOR_YELLOW}  -> Compiling via cargo build --release...${COLOR_NC}"
    cargo build --release --bin forgen
    cp "$SCRIPT_DIR/target/release/forgen" "$BIN_DIR/forgen"
    cp "$SCRIPT_DIR/target/release/forgen" "$BIN_DIR/datara"
    chmod +x "$BIN_DIR/forgen" "$BIN_DIR/datara"
    echo -e "${COLOR_GREEN}  -> Compilation finished and binaries installed.${COLOR_NC}"
fi

echo -e "${COLOR_YELLOW}[3/4] Installing Datara standard library...${COLOR_NC}"
if [ -d "$SCRIPT_DIR/stdlib" ]; then
    cp -r "$SCRIPT_DIR/stdlib/"* "$STDLIB_DIR/"
    echo -e "${COLOR_GREEN}  -> Installed stdlib to $STDLIB_DIR${COLOR_NC}"
fi

echo -e "${COLOR_YELLOW}[4/4] Configuring environment PATH...${COLOR_NC}"
PROFILE_FILE=""
if [ -n "$ZSH_VERSION" ] || [ -f "$HOME/.zshrc" ]; then
    PROFILE_FILE="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    PROFILE_FILE="$HOME/.bashrc"
elif [ -f "$HOME/.profile" ]; then
    PROFILE_FILE="$HOME/.profile"
fi

if [ -n "$PROFILE_FILE" ]; then
    if ! grep -q "DATARA_HOME" "$PROFILE_FILE" 2>/dev/null; then
        echo "" >> "$PROFILE_FILE"
        echo '# Datara Environment' >> "$PROFILE_FILE"
        echo 'export DATARA_HOME="$HOME/.datara"' >> "$PROFILE_FILE"
        echo 'export PATH="$HOME/.datara/bin:$PATH"' >> "$PROFILE_FILE"
        echo -e "${COLOR_GREEN}  -> Added Datara to $PROFILE_FILE${COLOR_NC}"
    else
        echo -e "${COLOR_GREEN}  -> Datara already configured in $PROFILE_FILE${COLOR_NC}"
    fi
fi

export PATH="$BIN_DIR:$PATH"
export DATARA_HOME="$INSTALL_DIR"

echo -e "\n${COLOR_GREEN}================================================================================${COLOR_NC}"
echo -e "${COLOR_GREEN} Verification & Environment Check:${COLOR_NC}"
echo -e "${COLOR_GREEN}================================================================================${COLOR_NC}"
"$BIN_DIR/forgen" --version || true
echo "DATARA_HOME: $INSTALL_DIR"
echo -e "\n${COLOR_CYAN}🎉 Datara & Forgen installed successfully!${COLOR_NC}"
echo -e "To start using it immediately, run:"
echo -e "  source $PROFILE_FILE"
echo -e "  forgen repl"
echo -e "  forgen doc --open\n"
