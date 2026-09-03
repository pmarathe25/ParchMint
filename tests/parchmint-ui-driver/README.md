# ParchMint UI driver

The UI driver exercises ParchMint's real desktop composition without creating
operating-system windows. It uses Iced's headless renderer to click and type in
the rendered widget tree, routes the resulting messages through the native
desktop update loop, and runs the production project and persistence services.

The `interaction-harness` feature contains all harness-only code and pulls in
the `iced_test` renderer. The production desktop does not enable this feature.

## Run the acceptance scenario

The first scenario creates a project, types in the custom editor, triggers the
60-second autosave boundary without sleeping, closes the project, relaunches
ParchMint, and opens the project from the recent-project list.

```console
cargo test -p parchmint-ui-driver --locked
```

## Drive the application from an agent

Start the JSON Lines driver with an isolated application-data directory. Each
input line is one command and each output line is one result.

```console
cargo run --locked -p parchmint-ui-driver -- \
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

The driver writes `failure.json` and a `failure-<renderer>.png` screenshot after
a command fails. `failure.json` contains the replayable user-action trace,
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
and absence of error diagnostics. They do not assert elapsed time or process
memory because the headless harness has no reliable host-level measurement API.
