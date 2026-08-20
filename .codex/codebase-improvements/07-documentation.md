# Documentation review

The documentation is broadly consistent with the code. Twelve source-backed
corrections remain.

1. `docs/architecture/architecture.md`: treat word counts as canonical
   `project.toml` manifest data, not `.parchmint/cache` data.
2. `docs/decisions.md`: describe contract bindings as checked-in code verified
   by schema checksums and regeneration diffs; `typify` is not a current build
   dependency.
3. `crates/parchmint-desktop/README.md`: describe the local diagnostic log as
   bounded/rotating at 1 MiB, not append-only.
4. `docs/product/project-model.md`: identify document word counts as canonical
   manifest data.
5. `docs/product/privacy-and-security.md`: require local diagnostics to contain
   only safe operational fields, never document prose, with bounded retention.
6. `docs/product/privacy-and-security.md`: state that path validation rejects
   symlink/reparse escapes for project and auxiliary storage.
7. `docs/product/release-gates.md`: add a gate for locked dependencies,
   advisories, licenses, provenance, notices, and SBOM verification.
8. `docs/product/features/save-recovery-and-closing.md`: SAVE-011 protects the
   latest completed durable save/checkpoint, not only autosave.
9. `docs/product/features/export.md`: title emission is tri-state; node page
   breaks are enabled or inherited.
10. `packaging/release.md`: use the canonical locked/offline `cargo run
    --package parchmint-ci -- ...` form and document notices/SBOM generation.
11. `packaging/release.md`: list `architecture verify` as a prerequisite and
    distinguish repository SBOM baseline verification from candidate SBOM
    verification.
12. `crates/parchmint-core-cli/README.md`: distinguish project filesystem paths
    (relative or absolute) from canonical project-relative resource paths.

The root README and other changed crate READMEs were checked without a confirmed
correction. This review used source and documentation inspection only; it did
not run documentation tooling or external link checks.
