#!/bin/sh
set -e

REPO="gokula-krishnan-r-dev/termedit-ai"
INSTALL_DIR="/usr/local/bin"
BINARY="termedit"

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map arch names
case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *)
    echo "Unsupported architecture: $ARCH"
    exit 1
    ;;
esac

# Map OS to archive name
case "$OS" in
  linux)
    ARCHIVE="termedit-linux-${ARCH}.tar.gz"
    ;;
  darwin)
    ARCHIVE="termedit-macos-${ARCH}.tar.gz"
    ;;
  *)
    echo "Unsupported OS: $OS"
    echo "Windows users: download from https://github.com/$REPO/releases"
    exit 1
    ;;
esac

# Get latest version
VERSION=$(curl -fsSL \
  "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')

DOWNLOAD_URL="https://github.com/$REPO/releases/download/v${VERSION}/${ARCHIVE}"

echo "Installing TermEdit v${VERSION} for ${OS}/${ARCH}..."

# Download + extract
TMP=$(mktemp -d)
curl -fsSL "$DOWNLOAD_URL" | tar xz -C "$TMP"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
else
  sudo mv "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
fi

chmod +x "$INSTALL_DIR/$BINARY"
rm -rf "$TMP"

echo "Done! Run: termedit --version"