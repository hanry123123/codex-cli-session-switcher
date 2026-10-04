# Upstream and fork details

- Upstream repository: https://github.com/openai/codex
- Source snapshot: Codex CLI 0.160.0, tag `rust-v0.160.0` (released 2026-10-01)
- Base commit: `a956835d020762cb2b570053af06f643a11c0ecc`
- Sidebar branch: `session-switcher-0.160.0`

This branch preserves the upstream `LICENSE` and `NOTICE`. The upstream Codex CLI source is Copyright OpenAI and distributed under the license included in this repository. Sidebar-specific changes are maintained in this public fork by `hanry123123`.

The sidebar changes span the TUI app lifecycle, event buffering and replay, transcript selection/copying, session list rendering and summary caching. Copy key handling preserves upstream 0.160.0 support for `Super+C`, `Control+C`, and `Control+Shift+C` terminal reports. The executable and local build caches are not checked into this source branch.
