# Deterministic corpus manifests

These files are generator inputs/results, not checked-in 10,000-node projects.
The generator emits documents on demand from a seed and word count:

```sh
cargo run -p parchmint-test-support --bin generate-corpus -- \
  --seed 20260720 --nodes 10000 --words 1000 \
  --manifest tests/fixtures/corpus/10000-nodes.toml
```

The committed manifests document the intended sizes and make stress runs
reproducible without inflating the source tree.
