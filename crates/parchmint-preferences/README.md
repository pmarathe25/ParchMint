# `parchmint-preferences`

**Purpose:** Store application-wide appearance, global dictionary, recent
projects, and shared settings. These preferences stay outside project files,
undo, recovery, History, and export styles.

## Interface

`PreferenceStore` loads and saves revisioned preferences.
`PreferenceCoordinator` implements `PreferenceService` and serializes changes
process-wide. `AppearanceController` implements `AppearanceService` and publishes
numbered `ThemeSnapshot` values. See [lib.rs](src/lib.rs).

## Preference writes

The coordinator checks the expected revision, applies one `PreferenceCommand`,
and calls `compare_and_save` to reject stale file revisions. The store writes a
versioned, deterministic temporary file, flushes it, and replaces the old file.
Only a durable replacement updates the in-memory snapshot and publishes
`PreferenceChange`.

An unreadable file is preserved and returned as a typed error. A failed save
leaves both durable and in-memory preferences unchanged and publishes no change.
Global dictionary updates use the same coordinator; spelling reloads words from
the preference store.

## Appearance events

An explicit System, Light, or Dark choice uses the expected preference revision.
After saving, the controller publishes the resolved Light or Dark theme. Failed
or stale writes leave the active theme unchanged.

In System mode, an OS appearance event publishes a new snapshot only if the
resolved theme changes; it does not rewrite preferences. Each UI frame uses one
complete immutable theme snapshot, applied across open windows and mapped to
[design-system](../parchmint-design-system/README.md) tokens.
