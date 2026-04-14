# Changelog

## Unreleased

### Added

- Bootstrapped a Rust workspace with a desktop shell prototype and a three-pane layout.
- Added live git repository tracking with a changed-file rail, diff mode, and file preview mode.
- Added context and model-usage telemetry ingestion from `.ghostty-shell/usage-events.jsonl`.
- Added 12 Rust tests covering git parsing, real repository snapshots, preview handling, and usage-event aggregation.

### Planned

- Embed the latest upstream Ghostty surface from `https://github.com/ghostty-org/ghostty`.
- Replace manual usage feeds with adapters that emit events automatically.
