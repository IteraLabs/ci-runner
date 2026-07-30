#!/bin/sh
set -eu

REPO="${CITOP_REPO:-IteraLabs/ci-runner}"
PREFIX="${CITOP_PREFIX:-$HOME/.local/bin}"
TAG="${CITOP_TAG:-latest}"

target=$(uname -m)-unknown-linux-gnu
case "$(uname -s)" in
  Linux) ;;
  *) echo "citop: Linux only, found $(uname -s)" >&2; exit 1 ;;
esac

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
