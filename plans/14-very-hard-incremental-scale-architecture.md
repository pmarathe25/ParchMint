# Stage 14: incremental scale architecture

Difficulty: very hard  
Recommended model: GPT-5.6 Sol, high or max reasoning  
Cost-conscious fallback: GPT-5.6 Terra, max reasoning with measured checkpoints after every slice  
Depends on: stages 11–13  
Master references: `PLAN.md` performance budgets; `plans/10-audit-report.md` §§4.4, 5.10–5.11, and 7 P2

## Outcome

Meet the 10,000-node / 10-million-word contract with bounded UI-thread work, incremental persistence/index/count/model updates, iterative graph algorithms, and cancellable streaming compile/export. Performance tests become enforced budgets rather than ignored measurements.

## Primary ownership

- `parchmint-domain` command/validation internals
- `parchmint-storage` dirty tracking and transaction boundaries
- `parchmint-index` incremental/background services
- Workspace projections, bridge worker orchestration, and `OutlineModel` deltas
- Markdown/compile traversal and ZIP/CRC hot paths
- Stress fixtures, benchmarks, profilers, and nightly performance gates

## Required work

### 1. Measure and freeze budgets

- Define reference hardware normalization and budgets for open, create/rename/reorder, save scheduling, first search, typing/count updates, model update, compile progress/cancel, and peak memory.
- Instrument UI-thread wall time separately from worker time and disk throughput.
- Record baselines on 100/1k/10k-node fixtures and representative 250k-word plus 10M-word documents before architecture changes.

### 2. Incremental domain and persistence

- Replace full-project clone/apply/validate with an in-place transaction that records a bounded inverse before mutation and rolls back on validation/persistence failure.
- Validate only the affected graph/style/document invariants while retaining an explicit full validator for open/migration/debug tests.
- Make ancestry, traversal, subtree clone/copy, and validation iterative with shared visited/depth structures; no user-controlled tree depth may recurse on the Rust or C++ stack.
- Track dirty manifests, documents, assets, and tombstones. A structural metadata edit must write only the necessary canonical files, never every Markdown body.
- Preserve atomicity across the small changed set with a documented transaction/recovery protocol and failure injection.

### 3. Incremental index and statistics

- Start cache rebuild/open off the UI thread and return first bounded results within the product budget; publish revisioned progress/deltas.
- Update word/character counts from editor block deltas, not whole QString copies. Cache document and subtree aggregates with ancestor delta propagation.
- Expose totals without triggering synchronous rebuilds; surface “indexing” explicitly when derived data is incomplete.
- Batch project-replace/search work and keep cancellation responsive within large individual documents.

### 4. Delta models and FFI boundaries

- Replace whole `BinderSnapshot`/`beginResetModel` refreshes with typed insert/remove/move/data-change deltas keyed by stable node ID.
- Cache roles in the C++ model; do not invoke the backend once per role per visible row.
- Separate content revision, structure revision, selection revision, and presentation/filter revision so background jobs invalidate only on relevant changes.
- Keep FFI payloads bounded: pass changed blocks/rows/counts rather than full documents/projects per interaction.

### 5. Streaming and cancellable compile/export

- Traverse iteratively and emit compile IR in bounded chunks or a spillable representation; avoid simultaneous full source/AST/IR/render/output copies.
- Add cancellation/progress checks inside Markdown parsing, inline/block rendering, asset reads, CRC, and archive writing.
- Replace bitwise CRC32 with a table/hardware-accelerated implementation and use checked ZIP64-capable output from stage 11.
- Define per-document and whole-project memory ceilings and prove cleanup on cancellation/failure.

### 6. Enforced performance gates

- Turn ignored stress tests into release-mode benchmark jobs with assertions and trend artifacts; run the expensive 10M-word/10k-node suite nightly.
- Add stack-depth adversarial fixtures and fail CI on recursion regressions.
- Add deterministic slow-disk/fault tests proving incremental saves do not acknowledge partial project state.

## Verification

- Use the canonical 10k-node corpus plus generated balanced/deep/wide trees and 10M words of mixed Unicode Markdown.
- Capture p50/p95 UI-thread latency, total time, files written/bytes written, allocations/peak RSS, FFI bytes, and cancellation latency.
- Compare every optimized command’s resulting canonical files and full-validator result against the reference implementation/property model.
- Run profiling on at least Linux and one of Windows/macOS; record platform filesystem differences.

## Acceptance gate

- A rename, synopsis/include edit, reorder, and single-document save perform bounded work independent of total document count and never rewrite unrelated bodies.
- No normal interaction performs a full project clone, full model reset, synchronous cold index rebuild, or per-keystroke full-document FFI scan.
- The 10k-node / 10M-word reference corpus meets all PLAN.md latency and memory budgets, including cancellation.
- Deep valid/invalid inputs fail or complete without stack overflow, panic abort, or superlinear depth behavior.
- Nightly performance gates are enabled with actionable regression output.

## Out of scope

- Distributed/cloud storage
- Real-time multi-user collaboration
- Replacing the open Markdown/TOML canonical format with a database

## Handoff

Create `docs/handoffs/14-incremental-scale-architecture.md`. Include before/after profiles, transaction/dirty-set design, revision taxonomy, model/index/count delta protocols, streaming/cancellation boundaries, exact benchmark commands/results, platform variance, and remaining headroom.
