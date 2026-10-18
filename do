#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

help() {
  cat <<'USAGE'
Usage: ./do <task> [args]

  test     Rust tests, then build the Python module and run pytest
  check    rustfmt --check, clippy with warnings as errors, ruff
  fmt      format Rust and Python
  bench    criterion benches, then Rust bindings vs pure Python
  demo     run `poh demo`; args pass through (./do demo --seconds 5)
  vectors  regenerate vectors/poh.json from plain hashlib loops
  help     show this list
USAGE
}

# pytest, maturin and ruff come from .venv, pinned by uv.lock.
py_env() { uv sync --quiet --locked --no-install-project; }

py_module() {
  py_env
  uv run --no-sync maturin develop --uv --release --quiet
}

# poh-py is a cdylib for Python; pytest covers it.
test_() {
  cargo test --workspace --exclude poh-py "$@"
  py_module
  uv run --no-sync pytest -q
}

check() {
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  py_env
  uv run --no-sync ruff format --check .
  uv run --no-sync ruff check .
}

fmt() {
  cargo fmt --all
  py_env
  uv run --no-sync ruff format .
  uv run --no-sync ruff check --fix .
}

bench() {
  cargo bench -p poh-core --bench poh "$@"
  py_module
  uv run --no-sync python scripts/bench.py
}

task="${1:-help}"
[[ $# -gt 0 ]] && shift
case "$task" in
  test) test_ "$@" ;;
  check) check ;;
  fmt) fmt ;;
  bench) bench "$@" ;;
  demo) cargo run --release --quiet -p poh-cli -- demo "$@" ;;
  vectors) python3 scripts/gen_vectors.py ;;
  help | -h | --help) help ;;
  *) echo "unknown task: $task" >&2; help >&2; exit 1 ;;
esac
