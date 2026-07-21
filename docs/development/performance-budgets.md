# Incremental-scale performance budgets

These are release gates, not debug-build expectations. The canonical reference
host is an Intel Core i7-8550U (4 cores/8 threads, up to 4.0 GHz), 8 GiB RAM,
local SSD, Linux x86-64. GitHub's `ubuntu-24.04` nightly runner is the permanent
Linux gate and must meet the same absolute limits. A platform report records its
filesystem and hardware; CPU normalization may be reported as
`host 10k-tree median / reference 10k-tree median`, but it never relaxes a UI
thread or cancellation limit.

| Interaction | Corpus | Budget | Measured boundary |
| --- | --- | ---: | --- |
| Binder usable after open | 10k nodes / 10M words | 3 s | owner/UI wall time; index continues on worker |
| Create, rename, reorder visible | 10k nodes | 100 ms | owner/UI wall time |
| Canonical durability | one changed set | 1 s | persistence-worker wall time |
| Editor delta/count update | 250k-word document | p95 16 ms, p99 50 ms | UI-to-Rust delta and ancestor propagation |
| Autosave scheduling | any | no UI block over 8 ms | owner/UI wall time |
| First bounded search | cold 10M-word cache | 300 ms | first revisioned batch/result; rebuild total is worker time |
| Model update | one row/subtree delta | 100 ms | Qt model signal handling; no reset |
| Compile cancellation | 10M words | 250 ms | token set to worker completion |
| Peak resident memory | reference corpus | 500 MiB | whole process RSS |

Storage gates additionally assert the exact `SaveMetrics` files/bytes count.
Rename or metadata changes write one Markdown file; reorder writes only
`outline.toml`; body save writes one Markdown file. Failure injection after each
canonical mutation must reopen to the complete old state.

Structural commands publish after owner-thread command validation, dirty-set
freezing, and model-delta preparation. TOML serialization, recovery backup,
`fsync`, and canonical replacement run on the serial project-save worker.
Transitions wait for acknowledgement; a failed worker transaction reverses the
optimistic command queue before the next canonical document save.

Run the enforced Rust budgets with:

```sh
cargo test --locked -p parchmint-app -p parchmint-markdown -p parchmint-index -p parchmint-compile -p parchmint-storage --release -- --nocapture
```

Run the native cached-role/model and editor benchmarks with:

```sh
cmake -S . -B build-stage14 -DCMAKE_BUILD_TYPE=Release
cmake --build build-stage14 --parallel
ctest --test-dir build-stage14 --output-on-failure
cmake --build build-stage14 --target editor-benchmark
```

The scheduled workflow captures `stage14-performance.txt` as a 90-day trend
artifact. The 10.02-million-word SQLite case is release-only; debug test runs
leave it ignored. Deep graph/projection tests remain enabled in ordinary CI so a
recursion regression fails before the nightly suite.
