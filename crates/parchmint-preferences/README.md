# `parchmint-preferences`

This crate stores application settings in an application preference file. These
settings include the appearance choice, global spelling dictionary, recent
projects, and settings shared by all windows. Project settings remain in the
project files.

The crate also stores the current Light or Dark theme for the running process.
When that theme changes, it publishes one numbered `ThemeSnapshot`. The UI
applies that snapshot to every open window. Appearance and global dictionary
changes do not enter project undo, project History, or export output.

## How it works

```text
preference command + expected revision
  -> one process-wide coordinator
  -> reject an outdated revision
  -> write the versioned preference file safely
  -> update the in-memory preference model
  -> publish the numbered preference change
  -> if the command sets appearance, resolve System, Light, or Dark
  -> publish the next numbered theme snapshot

operating-system appearance event while mode is System
  -> update the resolved system appearance
  -> if the resolved appearance changed, publish one numbered theme snapshot
```

The `System` choice follows operating-system appearance changes. The appearance controller publishes
the resolved Light or Dark choice; the UI maps it to design-system tokens.

## Interface

`PreferenceStore` loads and saves revisioned preferences.
`PreferenceCoordinator` implements `PreferenceService`; `AppearanceController`
implements `AppearanceService` and publishes numbered `ThemeSnapshot` values.

See [the source](src/lib.rs) for method signatures.

The UI receives one complete immutable `ThemeSnapshot` for each frame. It uses
that snapshot as a whole.

## Implementation

The preference file has a version and a deterministic encoding. Saving writes a
temporary file, flushes it, and replaces the old file. An unreadable file is
preserved for diagnosis and returned as a typed error; it is not overwritten
automatically. The store reports success only after the replacement is durable.

One `PreferenceCoordinator` serializes every preference change in the process.
It checks the caller's expected revision, applies one `PreferenceCommand`, and
uses `compare_and_save` to reject a stale file revision. A successful write
updates the in-memory snapshot and publishes one `PreferenceChange`. Appearance,
recent-project, and global-dictionary callers all use this coordinator.

Appearance and the global dictionary stay outside project state, project save,
recovery, and History.

For an explicit Light or Dark choice, `set_mode` sends the expected preference
revision to the coordinator. A successful preference write updates the
controller and publishes the next numbered theme snapshot. A stale revision or
save error leaves the active appearance unchanged. An operating-system
appearance event publishes a new snapshot only while the stored mode is
`System` and only when the resolved appearance actually changes; it does not
rewrite the preference file.

The global dictionary lives in the preference file and every update flows
through the same revision-checked coordinator; the spelling service reloads it
from the preference store. A preference save failure leaves the previous
durable settings and the in-memory snapshot in place; the caller receives the
typed error and no change is published.
