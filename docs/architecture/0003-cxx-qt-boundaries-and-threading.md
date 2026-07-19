# ADR-0003: CXX-Qt ownership, errors, and asynchronous work

Status: Accepted (Stage 01)

## Decision

CXX-Qt generates narrow QObject adapters. Bridge values are immutable snapshots,
stable identifiers, simple scalar properties, and explicit commands/results.
Rust owns mutable application state. C++ owns only Qt editor/platform adapters.
QML owns presentation state, never canonical files.

Blocking work runs on Rust workers. Each submission carries a project generation
and resource revision. A completion can affect current state only when both still
match. Closing a project increments generation; editing increments revision.
Cancellation is cooperative and stale completion rejection is mandatory even
when cancellation wins.

Expected failures cross the boundary as typed results and user-displayable error
signals. Panics and native termination are recorded locally; neither is a normal
error channel. No diagnostic is transmitted.
