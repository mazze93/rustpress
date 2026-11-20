#!/usr/bin/env bash
# Lint + test gate for the report reader (Rust/WASM) and its Python extractors.
# Mirrors rustpress CI: fmt check, clippy -D warnings, tests. Exits non-zero
# on the first failure. Usage: ./lint.sh        (check only)
#                              ./lint.sh --fix  (apply formatters first)
set -euo pipefail
cd "$(dirname "$0")"

RUFF_VERSION=0.13.0
ruff() { uvx --quiet "ruff@${RUFF_VERSION}" "$@"; }

if ! command -v cargo >/dev/null; then
  PATH="$(dirname "$(rustup which cargo)"):$PATH"
fi

if [[ "${1:-}" == "--fix" ]]; then
  cargo fmt
  ruff format .
  ruff check --fix .
fi

step() { printf '\n== %s\n' "$*"; }

step "rust: fmt";     cargo fmt --check
step "rust: clippy";  cargo clippy --locked --all-targets -- -D warnings
step "rust: clippy (wasm32)"
cargo clippy --locked --target wasm32-unknown-unknown --target-dir target -- -D warnings
step "rust: test";    cargo test --locked --quiet
step "python: ruff";  ruff check .
step "python: format"; ruff format --check .
step "python: pyright"; pyright
step "python: test";  python3 -I -m unittest discover -s extractors -q

printf '\n✓ all checks passed\n'
