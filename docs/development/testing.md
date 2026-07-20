# Build and test commands

Use the pinned Qt environment described in [`bootstrap.md`](bootstrap.md).
The Qt-free commands are useful for storage, Markdown, domain, indexing, and
compile work; the full commands also build the bridge and run offscreen CTest.

```sh
just bootstrap       # print toolchain versions
just format-check    # rustfmt and CMake configure probe
just lint            # Clippy and generated QML lint
just test-rust       # all Qt-free workspace tests
just test             # Rust tests, Qt build, and CTest
just smoke            # offscreen smoke tests
just bench-spikes     # ignored release-mode measurements
```

For the deterministic stress manifests:

```sh
cargo test -p parchmint-test-support
cargo run -p parchmint-test-support --bin generate-corpus -- \
  --seed 20260720 --nodes 10000 --words 1000
```

Do not commit generated 10,000-node corpora. Commit the seed, configuration,
and measured result instead. `git diff --check` is part of handoff review.
