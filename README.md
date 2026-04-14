# Ghostty Shell

[![CI](https://github.com/llirik0/ghostty-/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/llirik0/ghostty-/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Three-pane Rust shell around Ghostty:

- Left rail: live git change navigator for the current repo
- Center: terminal surface slot plus `Diff` and `Preview` modes
- Right rail: context and model-usage telemetry from JSONL events

The current app is a working shell prototype. The shell is live; Ghostty embedding is the next step, and it will target the latest upstream Ghostty repo:

- Upstream: `https://github.com/ghostty-org/ghostty`

## Current Features

- Rust desktop app built with `eframe`
- Live repo detection from the launch directory
- Changed-file list with status, add/remove counts, diff view, and file preview
- Usage panel that watches `.ghostty-shell/usage-events.jsonl`
- Status bar with repo, branch, cwd, focused file, total tokens, and mode

## Run

```bash
cargo run
```

Launch the app from inside the repo you want it to track.

## Test

```bash
cargo test
```

The suite covers the git snapshot parser, real temp-repo state collection, preview handling, and usage-event aggregation.

## Usage Events

The right rail watches `.ghostty-shell/usage-events.jsonl` by default. You can override it with `GHOSTTY_SHELL_USAGE_LOG`.

Each line should be JSON. This is enough:

```json
{"timestamp":"2026-04-14T15:10:00Z","provider":"OpenAI","model":"gpt-5.4","input_tokens":1200,"output_tokens":340,"cost_usd":0.09,"session":"shell-01"}
{"timestamp":"2026-04-14T15:12:00Z","provider":"Anthropic","model":"claude-sonnet","input_tokens":980,"output_tokens":210,"cost_usd":0.05,"session":"shell-01"}
```

Accepted aliases:

- `vendor` for `provider`
- `model_name` for `model`
- `prompt_tokens` for `input_tokens`
- `completion_tokens` for `output_tokens`
- `cost` for `cost_usd`
- `session_id` for `session`

## Next Moves

- Replace the center placeholder with a real Ghostty surface from the latest upstream repo
- Add multi-tab / multi-session handling
- Add command adapters that write usage events automatically instead of relying on manual JSONL feeds
