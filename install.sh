#!/bin/sh
set -eu

REPO="${CITOP_REPO:-IteraLabs/citop}"
PREFIX="${CITOP_PREFIX:-$HOME/.local/bin}"
TAG="${CITOP_TAG:-latest}"

case "$(uname -s)" in
  Linux) ;;
  *) echo "citop: Linux only, found $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  aarch64|arm64) arch=aarch64 ;;
  x86_64|amd64)  arch=x86_64 ;;
  *) echo "citop: no prebuilt binary for $(uname -m); build with cargo install --git https://github.com/IteraLabs/citop --locked" >&2; exit 1 ;;
esac
target="${arch}-unknown-linux-gnu"

if [ "$TAG" = latest ]; then
  base="https://github.com/${REPO}/releases/latest/download"
else
  base="https://github.com/${REPO}/releases/download/${TAG}"
fi

name="citop-${target}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "citop: downloading ${name} from ${REPO}"
curl -fsSL "${base}/${name}" -o "${tmp}/${name}"
curl -fsSL "${base}/${name}.sha256" -o "${tmp}/${name}.sha256"

echo "citop: verifying checksum"
( cd "$tmp" && sha256sum -c "${name}.sha256" )

mkdir -p "$PREFIX"
install -m 0755 "${tmp}/${name}" "${PREFIX}/citop"
echo "citop: installed to ${PREFIX}/citop"

case ":${PATH}:" in
  *":${PREFIX}:"*) ;;
  *) echo "citop: add ${PREFIX} to PATH" ;;
esac
