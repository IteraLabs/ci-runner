#!/bin/sh
set -eu

RUNNER_ROOT="${RUNNER_ROOT:-$HOME/actions-runner}"
CI_ROOT="${CI_ROOT:-$HOME/ci-toolchain}"
CI_CARGO="$CI_ROOT/cargo"
CI_RUSTUP="$CI_ROOT/rustup"
SERVICE="${RUNNER_SERVICE:-}"

say() { printf '%s\n' "$*"; }

if [ ! -d "$RUNNER_ROOT" ]; then
  say "no runner at $RUNNER_ROOT" >&2
  exit 1
fi

if [ -z "$SERVICE" ]; then
  SERVICE=$(systemctl list-units --type=service --all --no-legend 2>/dev/null \
    | awk '/actions\.runner\./{print $1; exit}')
fi

say "runner root:    $RUNNER_ROOT"
say "ci toolchain:   $CI_ROOT"
say "runner service: ${SERVICE:-none found}"

mkdir -p "$CI_CARGO" "$CI_RUSTUP"

changed=0

if ! grep -q '^CARGO_HOME=' "$RUNNER_ROOT/.env" 2>/dev/null; then
  [ -f "$RUNNER_ROOT/.env" ] && cp "$RUNNER_ROOT/.env" "$RUNNER_ROOT/.env.bak"
  printf 'CARGO_HOME=%s\nRUSTUP_HOME=%s\n' "$CI_CARGO" "$CI_RUSTUP" >> "$RUNNER_ROOT/.env"
  say "added CARGO_HOME and RUSTUP_HOME to .env"
  changed=1
else
  say "env already isolated"
fi

if ! grep -q "$CI_CARGO/bin" "$RUNNER_ROOT/.path" 2>/dev/null; then
  cp "$RUNNER_ROOT/.path" "$RUNNER_ROOT/.path.bak"
  printf '%s/bin:%s' "$CI_CARGO" "$(cat "$RUNNER_ROOT/.path")" > "$RUNNER_ROOT/.path.new"
  mv "$RUNNER_ROOT/.path.new" "$RUNNER_ROOT/.path"
  say "prepended $CI_CARGO/bin to .path"
  changed=1
else
  say "path already contains the ci toolchain"
fi

if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  say "host rustup missing, installing"
  rm -f "$HOME"/.cargo/bin/*
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile default --no-modify-path
else
  say "host rustup present"
fi

. "$HOME/.cargo/env"
rustup component add clippy rustfmt >/dev/null 2>&1 || true

if [ "$changed" = 1 ] && [ -n "$SERVICE" ]; then
  say "restarting $SERVICE"
  sudo systemctl restart "$SERVICE"
  sleep 5
fi

say ""
say "host cargo:     $(cargo --version)"
say "host toolchain: $(rustup show active-toolchain)"
say "ci CARGO_HOME:  $(grep '^CARGO_HOME=' "$RUNNER_ROOT/.env" | cut -d= -f2-)"
say "ci path head:   $(cut -c1-46 "$RUNNER_ROOT/.path")"
[ -n "$SERVICE" ] && say "runner:         $(systemctl is-active "$SERVICE")"
