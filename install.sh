#!/bin/sh
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
        if [ "$OS" = "macos" ]; then
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
if [ "$OS" = "windows" ]; then
    ASSET_SUFFIX="$OS-$ARCH.zip"
else
    ASSET_SUFFIX="$OS-$ARCH.tar.gz"
fi

echo "Detecting latest version..."
LATEST_RELEASE=$(curl -s https://api.github.com/repos/letmutex/gitas/releases/latest)
TAG_NAME=$(echo "$LATEST_RELEASE" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$TAG_NAME" ] || [ "$TAG_NAME" = "null" ]; then
    echo "Error: Could not find latest release version."
    exit 1
fi

echo "Latest version: $TAG_NAME"

# Construct download URL (assuming asset naming convention: gitas-vX.Y.Z-os-arch.tar.gz)
DOWNLOAD_URL="https://github.com/letmutex/gitas/releases/download/$TAG_NAME/gitas-$TAG_NAME-$ASSET_SUFFIX"
BIN_NAME="gitas"

# Windows specific handling (basic)
if [ "$OS" = "windows" ]; then
    echo "Downloading $DOWNLOAD_URL..."
    curl -L -o "gitas.zip" "$DOWNLOAD_URL"
    unzip -o "gitas.zip"
    echo "Extracted gitas.exe to current directory."
    echo "Please add this directory to your PATH or move gitas.exe to a folder in your PATH."
    rm "gitas.zip"
    exit 0
fi

# Unix specific handling (Linux/macOS)
INSTALL_DIR="$HOME/.gitas/bin"
mkdir -p "$INSTALL_DIR"

TEMP_DIR=$(mktemp -d)
echo "Downloading to $TEMP_DIR..."
curl -L -o "$TEMP_DIR/gitas.tar.gz" "$DOWNLOAD_URL"

echo "Extracting..."
tar -xzf "$TEMP_DIR/gitas.tar.gz" -C "$TEMP_DIR"

echo "Installing to $INSTALL_DIR..."
mv "$TEMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"
rm -rf "$TEMP_DIR"

# Path update logic
SHELL_TYPE="$(basename "$SHELL")"
PROFILE=""

case "$SHELL_TYPE" in
    zsh)
        PROFILE="$HOME/.zshrc"
        ;;
    bash)
        if [ "$OS" = "macos" ]; then
            PROFILE="$HOME/.bash_profile"
        else
            PROFILE="$HOME/.bashrc"
        fi
        ;;
esac

if [ -n "$PROFILE" ]; then
    if ! grep -q "$INSTALL_DIR" "$PROFILE"; then
        echo "Adding $INSTALL_DIR to PATH in $PROFILE"
        echo "" >> "$PROFILE"
        echo "# Gitas path" >> "$PROFILE"
        echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$PROFILE"
        printf "Please restart your terminal or run: \033[1msource %s\033[0m\n" "$PROFILE"
    fi
fi

echo "Successfully installed gitas $TAG_NAME to $INSTALL_DIR/$BIN_NAME"
