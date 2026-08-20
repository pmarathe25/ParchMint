# Storage/history bug audit

Scope: `parchmint-project-format`, `parchmint-project-repository`,
`parchmint-project-fs`, `parchmint-history-api`, and `parchmint-history-git2`.
This was source/test inspection only; no builds or tests were run.

## Confirmed bugs

### 1. `decode_project` accepts a project with no manifest

`ProjectFormatCodec::decode_project` (`crates/parchmint-project-format/src/lib.rs:1949-1981`)
only requires `format_control`, then iterates whatever happens to be in
`input.resources`. It never requires `project.toml` (or otherwise checks that a
complete project has its manifest). Therefore this succeeds:

```rust
let codec = ProjectFormatCodec::default();
let model = codec.decode_project(CanonicalInputSet {
    format_control: Some(b"1\n".to_vec()),
    resources: BTreeMap::new(),
});
assert!(model.is_ok()); // currently true
```

This also lets `migrate` accept the same incomplete snapshot and emit a
resource set containing only `.parchmint/format-version`, despite the format
README describing migration input/output as a complete canonical project.
Downstream callers that rely on this codec boundary can treat a missing
manifest as a valid project and lose the project definition instead of
reporting corruption.

Minimal fix: require the manifest resource after path validation (and return a
dedicated missing-required-resource/invalid-project error), before returning
`ProjectModel`; apply the same completeness rule to migration through
`decode_project`.

Focused regression test: add a `decode_project_rejects_missing_manifest`
test with format control plus an empty resource map and assert the new error;
also assert `migrate` rejects the same `SourceFormatSnapshot`.

### 2. In-memory repository serves documents after the project lease is closed

`InMemoryProjectRepository::load_document` (`crates/parchmint-project-repository/src/lib.rs:250-264`)
looks up `RepositoryState.active` and never checks that the corresponding
lease is still held. `Lease::drop` (`:91-99`) marks `state.leases[path]` false
but leaves `state.active` set. Reproduction:

```rust
let repo = InMemoryProjectRepository::new();
let opened = repo.create(request("project")).unwrap();
drop(opened);
assert_eq!(repo.load_document(DocumentId::new("doc")).unwrap(), b"body");
```

The call succeeds after the only `OpenProject` has been dropped. This violates
the lease lifetime modeled by `OpenProject::with_lease`, and differs from the
filesystem repository: its stale `ActiveProject` root fails lock-owner
verification after close. Tests using the in-memory implementation can thus
continue reading project data after the writable session has ended (and can
mask lifecycle bugs in callers).

Minimal fix: make `load_document` require an active, held lease (or clear
`active` in the lease drop path when it still refers to that project), and
return `RepositoryError::NotFound` when no session is open. Preserve the
existing behavior for a live lease.

Focused regression test: add `repository_rejects_document_load_after_lease_drop`,
create a project, drop its `OpenProject`, and assert
`Err(RepositoryError::NotFound { .. })`.

## Uncertain observations (not reported as confirmed defects)

- `FsProjectRepository` also retains `active` after lease drop, but its root
  capability fails lock verification, producing an integrity error rather than
  exposing bytes; this is stale-state/error-shaping evidence, not a data
  exposure.
- `SnapshotName::new` permits control characters, but History hex-encodes names
  in commit metadata, so the suspected newline/control injection did not
  reproduce as a parser or checkpoint corruption issue.
