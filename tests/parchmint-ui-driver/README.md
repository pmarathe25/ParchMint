# UI driver

**Purpose:** Exercise production widgets, the native update loop, and real
project services without OS windows. The driver uses bundled fonts, Iced's
headless renderer, and redraws between input events to preserve viewport layout.
The `interaction-harness` feature enables this code and `iced_test`; normal
desktop builds enable neither.

## Run workflows

```console
cargo test -p parchmint-ui-driver --locked -j 1
```

Successful persistence flows assert saved bytes and reopen projects. Each action
reports new failures from status messages, editor errors, dialogs, failed closes,
notifications, and inline History, search, and recovery errors. A test that injects
a failure must assert it; a successful click alone does not prove the operation.

Shared `create_project`, `create_group`, and `create_document` helpers use rendered
controls and production services. Keep custom input paths for keyboard and focus
regressions.

## Control time and completion order

The harness drains task work between actions by default. It does not run native
timer subscriptions or reproduce every OS scheduling interleaving.

| Control | Effect |
| --- | --- |
| `elapse_recovery_capture` | Trigger the recovery capture boundary |
| `advance_autosave_clock` | Advance autosave without sleeping |
| `elapse_notifications` | Dispatch notification expiry and other work due in that interval |
| `hold_completions` | Run services but retain their result messages |
| `release_completions(newest_first)` | Deliver retained results in either order after further input |

Use these controls to test stale results and timer boundaries without sleeps.
The JSON Lines command interface exposes the corresponding controls.

## Use the command driver

Each input line is a JSON command; each output line is its result. Use isolated
application data and a new artifact directory:

```console
cargo run --locked -j 1 -p parchmint-ui-driver -- \
  --app-root /tmp/parchmint-agent-run \
  --artifacts /tmp/parchmint-agent-run/failure
```

These commands inspect the launcher, open the creation form, fill its title,
and stop the driver:

```jsonl
{"command":"has_window","window":"launcher"}
{"command":"click_text","window":"launcher","text":"Create Project"}
{"command":"type_into","window":"launcher","placeholder":"Project title","value":"Flow Novel"}
{"command":"shutdown"}
```

Pass `--project <folder>` to open an existing disposable project. Commands such
as `type_at`, `elapse_autosave_idle`, and `active_editor_body` then exercise its
editor. See [main.rs](src/main.rs) for the complete command schema.

`contains_text` inspects the constructed widget tree. `text_is_visible` checks
the viewport and cached widget state. Follow visibility checks with a click and
result assertion when overlays could obstruct input. `snapshot` captures live
scroll, focus, and overlays as `<stem>-tiny-skia.png`, refusing existing output.

On failure, the driver writes `failure.json` and `failure-<renderer>.png`.
Numbered subdirectories keep later failures paired with their reports. Reports
contain the replayable action trace, production observations, and diagnostics;
traces record selectors and text lengths instead of document content.

## Reduce a UI failure to a regression

1. Reproduce the failure and retain its artifact bundle.
2. Find the first unexpected observation or diagnostic event to identify the
   responsible reducer, adapter, or service.
3. Reproduce only that boundary's input in a nearby unit or contract test and
   assert state or output directly.
4. Run the focused test and original reproduction. Remove the UI scenario if the
   focused test covers the same behavior. Retain a small integration test for
   rendering, focus, routing, or interactions across boundaries.

Choose the smaller regression deliberately: replaying an entire action trace
still exercises the full desktop stack. See [usability review](USABILITY.md) for
visual judgment and native verification.

## Large documents and performance

[large_document_resilience_flows.rs](tests/large_document_resilience_flows.rs)
covers 250,000-word documents through two-pane writing, workspace state, autosave,
global search and replacement, History, restart, and abandoned-session recovery.
Virtual clocks avoid real-time waits. Assertions cover retained text markers,
canonical files, recovered data, and absence of error diagnostics.

The opt-in `chapter_save_and_reopen_performance` test measures open, save, and
reopen for a 20,000-word chapter in an optimized build. The editor binding's
`chapter_authoring_performance` measures typing, selection, scrolling, chapter
switching, projection, and Linux process memory for eight chapters plus Research.
These report measurements, not machine-independent latency thresholds.
