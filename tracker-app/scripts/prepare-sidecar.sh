#!/usr/bin/env bash
#
# Build tracker-cli in release mode and copy it to the Tauri sidecar location.
# Tauri v2 expects externalBin entries to be suffixed with the host target
# triple so it can locate per-platform binaries at bundle time.
#
# Invoked automatically by `cargo tauri build` via `beforeBuildCommand` in
# tauri.conf.json; also safe to run manually.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${here}/.."

workspace_root="$(cd .. && pwd)"

(cd "${workspace_root}" && cargo build --release -p tracker-cli)

triple="$(rustc -vV | sed -n 's/^host: //p')"
if [[ -z "${triple}" ]]; then
    echo "could not determine host triple from rustc" >&2
    exit 1
fi

mkdir -p src-tauri/bin

# On Windows the binary has a .exe suffix; on Unix it does not.
if [[ -f "${workspace_root}/target/release/tracker-cli.exe" ]]; then
    cp -f "${workspace_root}/target/release/tracker-cli.exe" \
        "src-tauri/bin/tracker-cli-${triple}.exe"
    echo "sidecar staged at src-tauri/bin/tracker-cli-${triple}.exe"
else
    cp -f "${workspace_root}/target/release/tracker-cli" \
        "src-tauri/bin/tracker-cli-${triple}"
    echo "sidecar staged at src-tauri/bin/tracker-cli-${triple}"
fi
