#!/bin/sh
set -eu

version="${KEPUBIFY_VERSION:-v4.0.4}"
arch="$(uname -m)"

case "$arch" in
  x86_64|amd64)
    asset="kepubify-linux-64bit"
    ;;
  aarch64|arm64)
    asset="kepubify-linux-arm64"
    ;;
  armv7l|armv7)
    asset="kepubify-linux-arm"
    ;;
  armv6l|armv6)
    asset="kepubify-linux-armv6"
    ;;
  *)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

mkdir -p bin
url="https://github.com/pgaskin/kepubify/releases/download/${version}/${asset}"
echo "Downloading ${url}"
curl -L "$url" -o bin/kepubify
chmod +x bin/kepubify
bin/kepubify --version
