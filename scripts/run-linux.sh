#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${XRTRANSLATE_BINARY:-${ROOT_DIR}/dist/linux-x86_64/xrtranslate}"

cd "${ROOT_DIR}"

if [[ -x "${BINARY}" ]]; then
  exec "${BINARY}" "$@"
fi

if [[ ! -f "XR-Corpus/crates/core/Cargo.toml" ]]; then
  printf '%s\n' 'XR-Corpus is not initialized. Run: git submodule update --init XR-Corpus' >&2
  exit 2
fi

printf '%s\n' 'Release binary not found; running the Linux client through Cargo.' >&2
exec cargo run -p rust-client --release "$@"
