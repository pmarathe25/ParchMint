# V03 — `git2` history validation

**Disposition: pass.** Linux functional, fault, 250,000-checkpoint, and
1,000,000-checkpoint gates pass. Native Linux, Windows, and macOS 10,000
checkpoint smoke runs pass, and the same complete repository continued Linux
→ Windows → macOS with every enforced interchange invariant true. Select
`git2`; do not reopen `gix`.

## Validated composition

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
```

The lock resolves `libgit2-sys 0.18.7+1.9.6`. Runtime inspection reports
libgit2 1.9.6, `vendored = true`, threads and nanosecond timestamps enabled,
and HTTPS/SSH disabled. `LIBGIT2_NO_VENDOR` was unset. The release runner also
sets `LIBZ_SYS_STATIC=1`; `ldd` confirms no dynamic `libgit2` or `libz`
dependency.

## Functional result

The automated workload passed on Linux:

- Initialized a project-root repository with one app-managed `main`.
- Created autosave, structural, explicit-save, empty-tree named-snapshot, and
  additive restoration commits.
- Listed bounded pages with opaque resume cursors and scan budgets. ParchMint's
  linear `main` deliberately uses an unsorted revwalk, avoiding a global
  topological sort before every page.
- Filtered named snapshots and resource checkpoint metadata.
- Previewed and restored a document, group subtree, and whole project.
  Restores created new commits and never rewound `main`.
- Stored and validated deletion tombstone inputs, then restored the containing
  subtree.
- Detected both missing and corrupt reachable objects while current canonical
  project files remained independently readable.
- Normalized repository configuration to LF, no autocrlf, no executable-mode
  tracking, and no symlink tracking. History paths reject absolute paths and
  traversal.

## Scale result

Scale commits reuse an immutable representative tree to isolate commit/ref and
history-walk scaling; the functional phase separately exercises changing
blobs, trees, deletion, preview, and scoped restore.

| Measure | Static-link 10k smoke | Static-link 250k full |
|---|---:|---:|
| Completed checkpoints | 10,000 | 250,000 |
| Checkpoint p50 / p95 / p99 | 1.768 / 5.012 / 6.876 ms | 1.654 / 4.893 / 5.700 ms |
| 100-entry page p50 / p95 / p99 | 4.247 / 10.456 / 10.456 ms | 1.844 / 2.837 / 2.837 ms |
| Document / subtree / project restore | 3.181 / 3.059 / 3.292 ms | 8.973 / 3.406 / 10.525 ms |
| Reachable verification | 1.568 s | 32.236 s |
| Pack creation | 2.616 s | 23.630 s |
| Loose logical bytes before pack | 7,773,136 | 194,719,032 |
| Pack bytes | 977,255 | 24,613,228 |

The designated 1,000,000-checkpoint longevity run also completed on Linux:

- checkpoint p50/p95/p99: 0.698 / 0.825 / 1.384 ms over 999,992 samples;
- 100-entry page p50/p95/p99: 1.875 / 3.593 / 3.593 ms;
- document/subtree/project restore: 1.720 / 1.816 / 1.841 ms;
- reachable verification: 145.564 s; pack creation: 214.753 s;
- logical loose bytes at 1M: 779,504,650; final pack: 98,466,494 bytes;
- wall time: 19:32.19; peak RSS: 1,349,592 KiB; exit status 0.

The 11 MiB raw YAML contains all 999,992 checkpoint samples and has SHA-256
`ded1a056b4740e9970418aa1c85440ee9feaf6a52ac4221f8753e17f5cd62831`.

The raw files retain every checkpoint latency sample, every page sample,
quarter-scale disk/startup samples, loose/packed object counts, and exact
operation timings. Libgit2 can create a pack and index, but it does not provide
ParchMint's retention policy or a complete high-level `git gc` policy.
ParchMint must schedule maintenance, verify the new pack, and delete only
redundant unreachable/loose storage without pruning reachable checkpoints.

## Fault result

Five child processes were killed at instrumented boundaries: during a large
object write, during a large tree write, while a libgit2 reference transaction held
`main.lock`, inside pack progress, and inside object-database maintenance
traversal. In all cases:

- `main` remained at the last completed checkpoint;
- the repository reopened and reachable verification passed;
- current canonical files remained readable; and
- a subsequent checkpoint succeeded.

Interrupted reference transactions leave `.git/refs/heads/main.lock`. The
probe removes it only after restart and exclusive project ownership. That
small recovery rule belongs behind `HistoryStore`; it is not a reason to
replace `git2`.

## Interchange result

A complete 10,000-checkpoint Linux repository was archived and extracted on
Windows. Windows reopened it at source HEAD
`27275fc179cefaaef57c9d154ed072ff0f7574b9`, confirmed a clean worktree,
continued to `d9620e33f263192aed716d0b4d41e9a2824f06e7`, and verified reachable
history. That exact continued repository was archived and extracted on macOS,
which reopened at the Windows HEAD and continued to
`41d348f29ca7a47380e9700e11bbb5276a814047`.

Both continuation records report:

- source and copied HEAD equal;
- clean worktree after copy;
- only the intentional `project.toml` delta;
- Arabic/CJK plus decomposed combining-mark long path present; and
- reachable verification passing.

Workflow run `30352178340` enforced those booleans. Its Linux, Windows, and
final artifact digests are respectively
`ee6a797475d837cb19c45a61e48a1fef7c1605fbd9718d7534d710d9562faac2`,
`cad5497bd167cfab01e79ae9d5f16881f83420d1f4da9dd8db357bb3df02a79c`,
and `b8da577f99daa7a044b35d20b12422505a4cd5acdebde71da1897817e252a4bb`.

## Native 10k measurements

Each OS completed the functional checkpoint/browse/preview/restore/corruption
suite and 10,000-checkpoint scale smoke with vendored libgit2 1.9.6 and static
zlib. Values below are milliseconds.

| Platform | checkpoint p50/p95/p99 | page p50/p95/p99 | document/subtree/project restore | verify | pack |
|---|---:|---:|---:|---:|---:|
| Linux x86_64 | 0.796/0.854/0.888 | 1.591/1.732/1.732 | 1.583/1.619/1.735 | 283.610 | 1,164.541 |
| Windows x86_64 | 4.266/5.857/9.960 | 6.072/6.298/6.298 | 12.279/10.877/12.983 | 1,139.396 | 2,481.935 |
| macOS arm64 | 1.304/1.602/2.188 | 2.362/2.615/2.615 | 2.647/2.331/2.322 | 646.803 | 1,114.042 |

Linux and macOS each ended with 10,024 loose objects, one 977,255-byte pack,
and 10,053/10,040 files respectively. A path-string bug initially reported
zero Windows object counts despite successful pack creation. The corrected
component-based adapter was rerun natively in workflow `30351994494`: Windows
ended with 10,024 loose objects, one 977,255-byte pack, 10,053 files, and
9,033,115 total repository bytes. The corrected raw record supersedes only the
Windows stats/timing record; no history behavior changed.

## Hard-gate status

| Gate | Status | Evidence or gap |
|---|---|---|
| Exact local-only `git2` composition | Pass all three OSes | Vendored runtime true; HTTPS/SSH absent |
| Functional checkpoint/browse/preview/restore | Pass all three OSes | Native 10k smoke and focused tests |
| Missing/corrupt history isolation | Pass all three OSes | Both detected; canonical files readable |
| Interrupted write/tree/ref/pack/maintenance | Pass with bounded recovery | Stale ref lock needs exclusive-owner cleanup |
| 250,000 checkpoints | Pass on Linux | Full raw sample |
| 1,000,000 checkpoints | Pass on Linux | 1,000,000 completed; raw samples preserved |
| Windows native smoke/interchange | Pass | Corrected 10k stats plus enforced continuation |
| macOS native smoke/interchange | Pass | 10k smoke plus enforced continuation |
| Disk-full/permission/power-loss semantics | Not run | Platform fault harness required |
| Advisory/license automation | Conditional | Inventory exists; `cargo-audit`/`cargo-deny` unavailable |

## Recommendation and architecture delta

Select `git2` as the history backend. No concrete correctness, scale,
packaging, latency, or interchange failure justifies a fallback experiment;
do not implement `gix`.

`02-parchmint-architecture.md` should:

1. select `git2 =0.21.0` with only `vendored-libgit2`;
2. add the static-zlib release-build guard;
3. specify bounded unsorted paging over the one linear app-managed `main`;
4. specify exclusive-owner stale `main.lock` recovery after interrupted ref
   transactions;
5. keep pack scheduling, reachable verification, and loose-object cleanup as
   ParchMint-owned `HistoryStore` policy.

## Evidence

- `reports/raw/v03-git2-history.yaml`
- `reports/raw/v03-git2-history-full.yaml`
- `reports/raw/v03-git2-history-longevity.yaml`
- `reports/raw/v03-longevity-time.txt`
- `reports/raw/v03-git2-history-fault.yaml`
- `reports/raw/v03-git2-history-interchange.yaml`
- `reports/platform/v03-linux.md`
- `reports/platform/v03-fault-matrix.md`
- `reports/platform/v03-cross-platform-interchange.md`
- `reports/ci/Linux/v03-10k.yaml`
- `reports/ci-windows-stats/v03-10k.yaml`
- `reports/ci/macOS/v03-10k.yaml`
- `reports/ci/interchange-windows.yaml`
- `reports/ci/interchange-macos.yaml`
- `reports/supply-chain/v03-dependency-inventory.md`
- `reports/supply-chain/v03-NOTICES.md`
