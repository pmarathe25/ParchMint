# Stage 4D — UI/platform test simplification audit

Scope reviewed (source inspection only): tests in `parchmint-ui-api`,
`parchmint-ui-iced`, `parchmint-editor-iced`, `parchmint-design-system`,
`parchmint-platform-api`, and `parchmint-platform-native`.

## Safe simplifications

1. **Table-drive invalid external URL cases.**

   Evidence: `crates/parchmint-platform-api/tests/native_platform_api.rs:107-118`,
   `external_open_requires_a_validated_https_intent`, repeats five independent
   `assert!(ValidatedExternalIntent::https_url(...).is_err())` statements after
   one accepted HTTPS case. Replace those five statements with a small local
   array (the existing five literals) and `for url in invalid_urls {
   assert!(...is_err(), "{url}"); }`.

   This removes assertion boilerplate while preserving every rejected scheme,
   missing host, credentials, and malformed-percent-encoding vector, plus the
   accepted HTTPS check. Keep this test separate from native integration tests:
   it is the framework-neutral URL-construction contract.

2. **Use a local assertion macro/helper for the repeated stale-service checks.**

   Evidence: `crates/parchmint-platform-native/tests/native_platform.rs:295-333`,
   `stale_capabilities_reject_every_window_scoped_native_service`, repeats the
   same `block_on(...) == Err(PlatformError::stale_capability(stale))` shape for
   menu install, clipboard write/read, dialog selection, and external open.
   Replace only the repeated assertion shell with a local macro such as
   `assert_stale!(future_expr)` that evaluates the supplied future and compares
   it with the already-created `stale` capability error.

   Keep the five calls explicit (rather than hiding them in a heterogeneous
   collection): each call remains visible as coverage for a distinct native
   service and all window-authority rejection behavior is preserved. This is a
   mechanical reduction of duplicated assertion plumbing, not a merge with
   `worker_dispatch_is_nonblocking_and_revalidates_before_completion` or
   `replacing_a_generation_invalidates_the_old_capability_at_delivery`; those
   tests cover different timing/lifecycle races.

## Not recommended for simplification

- Do not merge the two save/recovery stress tests in
  `native_editor_save_recovery.rs:18-130`: one pauses each persistence boundary
  independently and checks acknowledgement details; the other pauses all
  boundaries simultaneously and checks queue collapse across a larger burst.
- Do not remove or merge the four ignored native smoke tests at
  `native_editor_iced.rs:478-492`: they are intentionally empty because they
  document separate Wayland, X11, macOS ARM, and Windows runners and their
  distinct prerequisites.
- Do not consolidate light/dark fixture or headless snapshot assertions in the
  Iced/editor tests. Those are the only rendering evidence for several fixtures;
  preserving the golden/reference checks is required.

No production behavior or test semantics should change. No builds or tests were
run, per assignment.
