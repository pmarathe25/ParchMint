# Backups and recovery

Autosave journals a complete Markdown body after the debounce interval and on
focus loss or clean shutdown. Records carry project generation, document
revision, and fingerprints, so stale completions cannot acknowledge newer work.
On restart, recovery offers preview, restore, discard, or save-copy; it never
silently applies a newer unsupported record.

Canonical saves use same-directory temporary files, flush, and atomic replace
where supported. Rotating backups preserve prior bodies, and migration creates
an idempotent pre-migration backup before changing canonical files. External
changes reload clean documents and present a conflict for dirty ones; overwrite
is never selected silently.
