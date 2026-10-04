#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

CODEX_SESSION_SWITCHER_BUILD=1 cargo build \
    --manifest-path "$repo_root/codex-rs/Cargo.toml" \
    --locked \
    --release \
    -p codex-cli \
    --features codex-tui/session-switcher \
    --bin codex

CODEX_SIDEBAR_BINARY="$repo_root/codex-rs/target/release/codex" \
    "$repo_root/scripts/install-codex-sidebar.sh"
