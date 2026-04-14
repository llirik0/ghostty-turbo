# Changelog

## Unreleased

### Added

- Bootstrapped a Rust workspace with a desktop shell prototype and a three-pane layout.
- Added live git repository tracking with a changed-file rail, diff mode, and file preview mode.
- Added context and model-usage telemetry ingestion from `.ghostty-shell/usage-events.jsonl`.
- Added 70 Rust tests covering app helpers, theme loading, git parsing, Ghostty bridge helpers, real repository snapshots, preview handling, and usage-event aggregation.
- Added a theme catalog that loads `themes/<name>/colors.toml`, applies a dark themed UI, and defaults to `tokyo-night`.
- Wired the center terminal pane into a real Ghostty bridge that detects local installs, focuses existing repo windows on macOS via AppleScript, and falls back to `open -na Ghostty.app --args ...` when automation fails.
- Added a real macOS Ghostty embed path that builds latest-upstream `GhosttyKit`, generates bindings in `build.rs`, and renders the terminal inside the shell window through a native child `NSView`.
- Added `scripts/bootstrap_ghostty_latest.sh` so the embed artifacts can be rebuilt locally against the latest Ghostty repo.

### Planned

- Pin a specific Ghostty commit once the in-shell embed path settles down and stop chasing upstream every single rebuild.
- Replace manual usage feeds with adapters that emit events automatically.
