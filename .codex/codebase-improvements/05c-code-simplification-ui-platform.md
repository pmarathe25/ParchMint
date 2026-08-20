# Stage 5c — UI/platform code simplification audit

Scope: production code in `parchmint-ui-api`, `parchmint-ui-iced`,
`parchmint-editor-iced`, `parchmint-design-system`, `parchmint-platform-api`,
and `parchmint-platform-native`, including the current session/window authority
and stale/canceled task outcome changes.

## Result

No high-confidence simplification is recommended (0 changes). The inspected
paths have active consumers or deliberately preserve authority, cancellation,
native-effect, rendering, or thread guarantees. No source files were edited.

## Checked exceptional case

`crates/parchmint-ui-iced/src/design_tokens.rs:14,20-21` defines
`TOKEN_SOURCE_SHA256`, `CONTROL_HEIGHT`, and `CORE_ICON_SIZE`; repository-local
search found no current consumers beyond their declarations. They are public
constants in the public `design_tokens` module, however, so removing them would
break the crate's public API despite having no in-repository use. They are not
recommended for this stage; deprecation/removal would require an explicit API
decision.

The apparent layering in `ProjectUiAccess`/`ProjectUiPorts`
(`crates/parchmint-ui-api/src/lib.rs:645-851`) is the session-authority boundary,
and the registry/completion split in
`crates/parchmint-platform-native/src/registry.rs:42-87` is required to
authorize immediately before work and re-check before delivery. The task
outcome distinctions in `crates/parchmint-ui-iced/src/native.rs:711-781` are
also behaviorally meaningful for stale/canceled versus user-visible failures.

## Validation

Source/config inspection only, as requested. No builds, tests, metadata, or
heavy scans were run.
