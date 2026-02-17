#!/bin/bash
set -e

# Determine OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        OS="linux"
        ;;
    Darwin)
        OS="macos"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        OS="windows"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)
        ARCH="x64"
        ;;
    arm64|aarch64)
        if [ "$OS" == "macos" ]; then
            ARCH="arm64"
        else
            echo "Unsupported architecture: $ARCH on $OS"
            exit 1
        fi
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Construct asset name pattern
# Using regex for version flexibility since we fetch the latest release tag
if [ "$OS" == "windows" ]; then
    ASSET_SUFFIX="$OS-$ARCH.zip"
else
    ASSET_SUFFIX="$OS-$ARCH.tar.gz"
fi

echo "Detecting latest version..."
LATEST_RELEASE=$(curl -s https://api.github.com/repos/letmutex/gitas/releases/latest)
TAG_NAME=$(echo "$LATEST_RELEASE" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$TAG_NAME" ] || [ "$TAG_NAME" == "null" ]; then
    echo "Error: Could not find latest release version."
    exit 1
fi

echo "Latest version: $TAG_NAME"

# Construct download URL (assuming asset naming convention: gitas-vX.Y.Z-os-arch.tar.gz)
DOWNLOAD_URL="https://github.com/letmutex/gitas/releases/download/$TAG_NAME/gitas-$TAG_NAME-$ASSET_SUFFIX"
INSTALL_DIR="/usr/local/bin"
BIN_NAME="gitas"

# Windows specific handling (basic)
if [ "$OS" == "windows" ]; then
    echo "Downloading $DOWNLOAD_URL..."
    curl -L -o "gitas.zip" "$DOWNLOAD_URL"
    unzip -o "gitas.zip"
    echo "Extracted gitas.exe to current directory."
    echo "Please add this directory to your PATH or move gitas.exe to a folder in your PATH."
    rm "gitas.zip"
    exit 0
fi

# Unix specific handling (Linux/macOS)
TEMP_DIR=$(mktemp -d)
echo "Downloading to $TEMP_DIR..."
curl -L -o "$TEMP_DIR/gitas.tar.gz" "$DOWNLOAD_URL"

echo "Extracting..."
tar -xzf "$TEMP_DIR/gitas.tar.gz" -C "$TEMP_DIR"

echo "Installing to $INSTALL_DIR..."
# Request sudo if not root and trying to install to system directory
if [ "$EUID" -ne 0 ] && [ -w "$INSTALL_DIR" ]; then
    mv "$TEMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
elif [ "$EUID" -ne 0 ]; then
    echo "Sudo permission required to install to $INSTALL_DIR"
    sudo mv "$TEMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
else
    mv "$TEMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
fi

chmod +x "$INSTALL_DIR/$BIN_NAME"
rm -rf "$TEMP_DIR"

echo "Successfully installed gitas $TAG_NAME to $INSTALL_DIR/$BIN_NAME"
