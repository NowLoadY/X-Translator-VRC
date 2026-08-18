#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${XRTRANSLATE_RELEASE_DIR:-${ROOT_DIR}/dist/linux-x86_64}"
FEATURES="${XRTRANSLATE_FEATURES:-}"

cd "${ROOT_DIR}"

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  printf '%s\n' 'This release script targets Linux x86_64.' >&2
  exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
  printf '%s\n' 'cargo is required; install Rust 1.95 or newer.' >&2
  exit 127
fi

if [[ ! -f "XR-Corpus/crates/core/Cargo.toml" ]]; then
  printf '%s\n' 'XR-Corpus is not initialized. Run: git submodule update --init XR-Corpus' >&2
  exit 2
fi

cargo_args=(build -p rust-client --release)
if [[ -n "${FEATURES}" ]]; then
  cargo_args+=(--features "${FEATURES}")
fi
cargo "${cargo_args[@]}"

rm -rf "${TARGET_DIR}"
mkdir -p "${TARGET_DIR}/resources"
install -m 0755 "target/release/rust-client" "${TARGET_DIR}/xrtranslate"
install -m 0644 config.json "${TARGET_DIR}/config.json"
cp -a rust-client/resources/. "${TARGET_DIR}/resources/"

printf 'Linux release staged at %s\n' "${TARGET_DIR}"
