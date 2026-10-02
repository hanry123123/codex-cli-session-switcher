# Codex CLI session switcher

This source checkout builds a separate `codex-sidebar` command from Codex CLI 0.157.0. It keeps
the existing `codex` installation untouched.

## Keys

- `F6`: focus the left session list; press `F6` again to return to the composer.
- `Up` / `Down`: select a session. The selected row shows a short summary and the latest user
  request underneath it.
- `Enter` in the list: open the selected session. After returning to the composer with `F6`, an
  empty-composer `Enter` opens the selected session.
- `F7`: switch to the other of the two most recently visited sessions.

Switching reuses Codex's app-server resume/view path. It does not submit a turn interrupt to the
session being left, so its running generation continues while another session is displayed.

## Build and install

Run `scripts/build-codex-sidebar.sh`. It builds the feature-enabled CLI, copies the complete
installed Codex package into `~/.local/lib/codex-sidebar`, replaces only that package's CLI
executable, and installs `~/.local/bin/codex-sidebar`. The launcher uses the normal shared
app-server path. It reuses companion tools from the installed Codex package and leaves the existing
`codex` installation untouched. Installation stops if the destination belongs to another command.

To build without installing:

```sh
cd codex-rs
CODEX_SESSION_SWITCHER_BUILD=1 cargo build --release -p codex-cli --features codex-tui/session-switcher --bin codex
```

The binary is at `codex-rs/target/release/codex`. To install an already built binary without
rebuilding, run `scripts/install-codex-sidebar.sh`.

## Summaries

When signed in with a ChatGPT account, the switcher uses `gpt-6-luna` through Codex's existing
subscription authentication. It never reads an API key for summaries. If subscription
authentication or the model is unavailable, rows fall back to the session title and preview.
Summaries are cached under `$CODEX_HOME/session-switcher/summaries-v1.json` (or the default Codex
home when `CODEX_HOME` is unset). The first scan summarizes saved sessions; completed turns queue
incremental updates after three new rounds. Reopening the list refreshes the thread inventory and
catches up on sessions whose notifications were not attached while they ran.
