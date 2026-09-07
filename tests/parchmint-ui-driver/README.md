# ParchMint UI driver

The UI driver exercises ParchMint's real desktop composition without creating
operating-system windows. It loads the desktop's bundled fonts and delivers
redraw notifications between input events so panes wrap to their allocated
viewport. It uses Iced's headless renderer to click and type in
the rendered widget tree, routes the resulting messages through the native
desktop update loop, and runs the production project and persistence services.

The `interaction-harness` feature contains all harness-only code and pulls in
the `iced_test` renderer. The production desktop does not enable this feature.

## Completion and failure checks

Every action reports new failure status messages, editor errors, error dialogs,
failed closes, error notifications, and inline History, search, and recovery errors. A test that injects a failure must assert the returned error;
clicking a control alone does not establish success. Successful flows check
saved bytes and reopen the project to verify persistence.

The harness drains task work between actions by default. It does not run native
timer subscriptions or reproduce every OS scheduling interleaving. Use
`elapse_recovery_capture`, `advance_autosave_clock`, and `elapse_notifications`
for timer boundaries. Notification expiry dispatches the production timer and
also runs any other work due during that interval.
`hold_completions` runs service work while retaining result messages;
`release_completions(newest_first)` delivers them in either order after more
user input. These controls exercise stale snapshots and delayed UI updates
without sleeps. The JSON Lines driver exposes the same commands.

Shared `create_project`, `create_group`, and `create_document` helpers use rendered
controls and the real service graph. Specialized keyboard and focus tests retain
their own input paths.

## Run the acceptance scenarios

The first scenario creates a project, types in the custom editor, triggers the
60-second autosave boundary without sleeping, closes the project, relaunches
ParchMint, and opens the project from the recent-project list.

```console
cargo test -p parchmint-ui-driver --locked -j 1
```

## Drive the application from an agent

Start the JSON Lines driver with an isolated application-data directory. Each
input line is one command and each output line is one result.

```console
cargo run --locked -j 1 -p parchmint-ui-driver -- \
  --app-root /tmp/parchmint-agent-run \
  --artifacts /tmp/parchmint-agent-run/failure
```

Example commands:

```jsonl
{"command":"click_text","window":"launcher","text":"Create Project"}
{"command":"type_into","window":"launcher","placeholder":"Project title","value":"Flow Novel"}
{"command":"type_at","window":"project","x":500.0,"y":300.0,"value":"Hello"}
{"command":"elapse_autosave_idle"}
{"command":"active_editor_body"}
{"command":"shutdown"}
```

`contains_text` checks text in the constructed widget tree. `text_is_visible`
checks the current viewport and cached widget state. Neither establishes that an
overlay leaves the control usable; follow up with a click and assert its result.
`snapshot` renders the live widget cache, preserving scroll, focus, and overlays.
It writes `<stem>-tiny-skia.png` and refuses an existing file.

Use the [agent usability review](USABILITY.md) to judge clarity and layout with
these tools and a native application run.

The driver writes `failure.json` and a `failure-<renderer>.png` screenshot after
a command fails. Later failures in the same run use numbered subdirectories so
each screenshot stays paired with its own report. `failure.json` contains the replayable user-action trace,
production boundary observations, and structured diagnostics. The trace records
text lengths and selectors, not document content.

## Harden a UI bug into a focused test

Use the failure bundle to find the lowest component that reproduces the bug.

1. Reproduce the bug with the UI driver and keep `failure.json`.
2. Find the first unexpected production observation or diagnostic event. Its
   target and operation name identify the reducer, adapter, or service boundary
   to test.
3. Recreate only that boundary's input in a colocated unit or contract test.
   Assert the resulting state or output directly.
4. Run the focused test and the original UI reproduction. If the focused test
   covers the same product decision, keep the focused test and remove the UI
   scenario. Keep a small UI test when the bug depends on rendering, focus,
   event routing, or several boundaries working together.

This reduction is intentionally a developer decision. A user-action trace can
be replayed automatically, but automatic conversion would preserve the full
desktop stack and would still be an end-to-end test.

## Large-document authoring resilience

`tests/large_document_resilience_flows.rs` exercises the supported
250,000-word document size through real desktop composition. Its flows cover
two-pane authoring workspace state and autosave, project-wide search and
replacement, History loading, restart, and recovery after an abandoned
session. They use the harness's virtual clocks, so a long writing session is
reproducible without sleeping in CI.

The flows assert retained markers, canonical file contents, recovery results,
and absence of error diagnostics. The opt-in
`chapter_save_and_reopen_performance` test measures wall-clock open, save, and
reopen times for a 20,000-word chapter in an optimized build. The editor
binding's `chapter_authoring_performance` test measures typing, selection,
scrolling, chapter switching, projection time, and Linux process memory for
eight chapters plus Research. These measurements are reported, not
machine-independent pass/fail latency thresholds.
