# DRY synthesis

The subsystem reviews found nine small consolidations worth implementing:

1. Centralize empty `SaveQueue` construction.
2. Centralize recovery inventory/status refresh.
3. Put canonical byte hashing on `ContentHash` and use it at raw byte-hash sites.
4. Share the typed decode/check/re-encode sequence inside contract validation.
5. Share project-session authorization and stale-error construction.
6. Share stable 16-byte ID hex formatting inside `parchmint-ui-iced`.
7. Share Linux native-menu handle compatibility matching.
8. Share core CLI project-root acquisition while keeping lease ownership visible.
9. Express homogeneous release evidence checks as local tables.

Do not introduce a shared preferences/workspace atomic-write helper in this
stage. Those two implementations have no existing common persistence owner, so
consolidation would create a new dependency boundary for only two callers.

The selected changes are local or type-owned, preserve current public behavior,
and reduce realistic divergence risk without adding a utility crate.
