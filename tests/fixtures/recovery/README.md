# Recovery fixture catalog

`recovery-v1.toml` is a complete format-1 record with a stamped revision,
fingerprints, and Unicode body. It is intentionally small; recovery tests
should create records through the application lifecycle and use this file for
parser/forward-version cases.
