# Stage 6 DRY audit: core and storage

Scope reviewed: production code in `parchmint-domain`, `parchmint-application`,
`parchmint-editor-api`, `parchmint-editor-core`, `parchmint-save`,
`parchmint-recovery-*`, `parchmint-project-format`, `parchmint-project-repository`,
`parchmint-project-fs`, and `parchmint-history-*`. No implementation changes were
made. The following are high-confidence consolidations (three maximum requested).

## 1. `EditorPersistenceCoordinator` constructs the same empty queue twice

- Evidence: `crates/parchmint-application/src/editor_persistence.rs`,
  `EditorPersistenceCoordinator::new` (around lines 85-91) and
  `EditorPersistenceCoordinator::new_recovery_only` (around lines 102-108) each
  inline the identical `SaveQueue { latest: None, in_flight: Default::default(),
  max_depth: 0, submitted: 0, coalesced: 0 }`.
- Risk: adding a queue field or changing its initial invariants requires two
  edits; the constructors can silently diverge even though both expose the same
  coordinator behavior.
- Smallest solution: add a private `SaveQueue::new() -> Self` (or a private
  `empty_queue()` function) and call it from both constructors. Keep the
  constructor-specific recovery coordinator selection unchanged.
- Validation: existing constructor/status and save-queue contract tests should
  continue to assert both construction paths; no public API changes are needed.

## 2. Recovery inventory status synchronization is repeated in four workflows

- Evidence: `crates/parchmint-application/src/editor_persistence.rs` repeats the
  same two assignments, `status.recovery_retained_records = inventory.records.len()`
  and `status.recovery_inventory = Some(inventory)`, in
  `persist_projection` (around lines 132-134),
  `persist_projection_with_document_hash` (177-179),
  `reconcile_recovery` (199-200), and `discard_reconciled_recovery` (230-231).
  Each site obtains `self.recovery.recovery_inventory()` immediately beforehand.
- Risk: a future inventory/status field can be updated in some lifecycle paths
  but omitted in another, producing stale recovery UI/state. The repeated code
  already has identical semantics; the surrounding state transitions remain
  intentionally different.
- Smallest solution: add a private `refresh_recovery_status(&self,
  status: &mut EditorPersistenceStatus) -> Result<(), EditorPersistenceError>`
  that fetches one inventory and assigns both fields. Call it while each existing
  status guard is held. For `reconcile_recovery`, retain its separate isolation
  assignment in the same guard.
- Validation: exercise the existing persistence/recovery contract tests for all
  four operations and verify retained-record count and inventory after each.

## 3. Raw SHA-256 byte hashing is reimplemented instead of living on `ContentHash`

- Evidence: the same `ContentHash::from_bytes(Sha256::digest(bytes).into())`
  operation appears in `parchmint-project-format/src/lib.rs` (around lines 538,
  585, 2071), `parchmint-application/src/project_persistence.rs` (1683 and
  1816), `parchmint-history-git2/src/lib.rs` (317 and 887), and
  `parchmint-editor-api/src/lib.rs::content_hash` (1143-1145). These are all
  hashes of one byte slice; the multi-input recovery receipt/content hash
  routines are intentionally different and should not be folded in.
- Risk: callers repeat the digest-to-`ContentHash` conversion and each crate
  carries `sha2` imports for this one canonical operation. A future hash-policy
  change (or accidental use of a different conversion) can diverge across
  format, editor, application, and History validation paths.
- Smallest solution: add `ContentHash::of_bytes(bytes: &[u8]) -> Self` beside
  `ContentHash::from_bytes` in `parchmint-project-format`, then replace only the
  raw byte-hash call sites above. This keeps the type owner authoritative and is
  not a general-purpose utility crate; retain `Sha256` where callers build
  domain-specific multi-input identities.
- Validation: run format, editor-api, application, and History contract tests;
  compare hashes against current `as_bytes()` values and test empty bytes.
