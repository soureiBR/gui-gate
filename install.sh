#!/bin/bash
# SoureiGate Installer — Linux/macOS
# Uso: curl -fsSL https://github.com/SEU_ORG/soureigate/raw/main/install.sh | bash

set -e

echo "=== SoureiGate Installer ==="
echo ""

REPO="soureiBR/gui-gate"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/soureigate"

mkdir -p "$INSTALL_DIR"
mkdir -p "$CONFIG_DIR"

# Detecta OS + arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)  PLATFORM="linux" ;;
    darwin) PLATFORM="macos" ;;
    *)      echo "OS não suportado: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64)  ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)       echo "Arch não suportado: $ARCH"; exit 1 ;;
esac

FILENAME="gate-${PLATFORM}-${ARCH}"
URL="https://github.com/${REPO}/releases/latest/download/${FILENAME}"

echo "Baixando ${FILENAME}..."
curl -fsSL "$URL" -o "$INSTALL_DIR/gate"
chmod +x "$INSTALL_DIR/gate"

echo "Instalado em $INSTALL_DIR/gate"

# Verifica PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    SHELL_RC=""
    if [ -f "$HOME/.zshrc" ]; then
        SHELL_RC="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        SHELL_RC="$HOME/.bashrc"
    fi

    if [ -n "$SHELL_RC" ]; then
        echo "" >> "$SHELL_RC"
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_RC"
        echo "PATH adicionado em $SHELL_RC"
        echo "Rode: source $SHELL_RC"
    else
        echo ""
        echo "⚠ Adicione ao seu shell rc:"
        echo '  export PATH="$HOME/.local/bin:$PATH"'
    fi
fi

echo ""
echo "✓ Pronto! Digite 'gate' para abrir o SoureiGate."
echo ""
