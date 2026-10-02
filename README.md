# Codex CLI Session Switcher

A public fork of [OpenAI Codex CLI](https://github.com/openai/codex) with a persistent session sidebar. It builds a separate `codex-sidebar` executable, leaving the ordinary `codex` command in place.

This branch is pinned to Codex CLI 0.157.0. It includes the session switcher, cached conversation summaries, mouse-wheel scrolling, macOS `Command+C` text copying, and a fix for replaying buffered transcript messages that are missing from a read-only history snapshot.

## Shortcuts

- `F6`: focus the sidebar; press again to return to the composer.
- `Up` / `Down`: select a session.
- `Enter`: open the selected session.
- `F7`: switch between the two most recently visited sessions.
- The first sidebar row starts a new conversation.

Each saved session row shows a summary and a smaller preview of the latest user request. Hover over the list and scroll with the mouse or trackpad to move through it.

## Build and install

On macOS with the Rust toolchain selected by this checkout and a complete Codex 0.157.0 standalone package installed, run:

```sh
./scripts/build-codex-sidebar.sh
```

The script builds the feature-enabled CLI and installs it as `~/.local/bin/codex-sidebar`. The installer copies the matching Codex companion tools into an isolated package and refuses to replace an unmanaged command or use a package version that does not match the binary. For a build without installation:

```sh
cd codex-rs
CODEX_SESSION_SWITCHER_BUILD=1 cargo build --locked --release -p codex-cli --features codex-tui/session-switcher --bin codex
```

The binary is written to `codex-rs/target/release/codex`. To install an existing build, use `scripts/install-codex-sidebar.sh` and set `CODEX_SIDEBAR_BINARY` and, if needed, `CODEX_SIDEBAR_BASE_PACKAGE`.

## Summaries and session data

When signed in, summary generation uses Codex's existing ChatGPT subscription authentication with `gpt-6-luna`; conversation text is sent to that model to create the summary. Summaries are cached under `$CODEX_HOME/session-switcher/summaries-v1.json`. If authentication or the model is unavailable, rows fall back to the session title and preview. See [SESSION_SWITCHER.md](SESSION_SWITCHER.md) for details.

Session switching uses Codex's app-server resume/view path and does not send a turn-interrupt request. The behavior of a live generation after switching away has not been verified end to end against a signed-in account.

## Verification

The focused read-only history replay regression passed on the release build:

```sh
just test --release -p codex-tui --features codex-tui/session-switcher --locked --lib read_only_snapshot_replays_missing_items_without_replaying_loaded_transcript
```

The complete feature-enabled TUI test suite has not been verified as green. See [UPSTREAM.md](UPSTREAM.md) for the exact base revision and the [Codex upstream repository](https://github.com/openai/codex).
