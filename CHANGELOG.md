# Changelog

## Unreleased

### Added

- Bootstrapped a Rust workspace with a desktop shell prototype and a three-pane layout.
- Added live git repository tracking with a changed-file rail, diff mode, and file preview mode.
- Added context and model-usage telemetry ingestion from `.ghostty-shell/usage-events.jsonl`.
- Added 52 Rust tests covering app helpers, theme loading, git parsing, real repository snapshots, preview handling, and usage-event aggregation.
- Added a theme catalog that loads `themes/<name>/colors.toml`, applies a dark themed UI, and defaults to `tokyo-night`.

### Planned

- Embed the latest upstream Ghostty surface from `https://github.com/ghostty-org/ghostty`.
- Replace manual usage feeds with adapters that emit events automatically.
