# Version 1 threat model

## Protected assets and trust boundaries

The primary asset is complete, readable canonical project data. Project files,
external attachment sources, Markdown/YAML/TOML, export destinations, and files
changed by other programs are untrusted. QML cannot access project storage;
validated Rust services own paths and writes. SQLite and `.parchmint/` are
disposable or recoverable state, never authority.

## Defenses

- Relative paths reject absolute paths, traversal, and symlink components.
- Attachment import rejects symlinks, devices, oversized input, collisions, and
  active in-process execution. External opening requires confirmation.
- Parsers bound document/front-matter size and nesting and retain unsupported
  Markdown as visible opaque source.
- Canonical and export writes use same-directory temporary files, flushing, and
  atomic replacement; migration, replacement, and autosave retain recovery data.
- Revisions and project generations reject stale save/index/export completion.
- Project replacement previews recheck all source fingerprints before the first
  write, back up originals, roll back partial writes, and conflict-protect undo.
- HTML export rejects active script output; DOCX/EPUB package writers validate
  their fixed entry sets and never accept caller-controlled archive paths.
- ParchMint declares no direct Qt Network module and contains no network client;
  Qt QML has a transitive Qt Network dependency which is audited as platform
  infrastructure and is not invoked by application code.

## Residual risks

Other applications used to open attachments or exports are outside the process
boundary. Local administrators, compromised operating systems, hostile storage
drivers, and physical attackers are out of scope. Change fingerprints detect
conflicts but are not cryptographic authenticity checks. Screen-reader,
installer, signer, and consumer-format behavior must still be validated on each
physical release platform.
