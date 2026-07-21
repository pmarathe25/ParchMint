# ParchMint recovery record 1

> Read when changing autosave journals, recovery discovery, preview, restore,
> discard, save-copy, or save acknowledgement.

Recovery is local derived state, not canonical project format.

Each dirty open document has at most one atomic record at
`.parchmint/recovery/<document-id>.toml`. The record is replaced after the
750 ms default debounce and immediately on focus loss or clean shutdown.

| Field | Meaning |
|---|---|
| `format_version` | Recovery schema version; value is `1` |
| `project_generation` | Open-project incarnation captured with the edit |
| `document_id` | Stable canonical document identity |
| `revision` | Monotonic document revision represented by `body` |
| `base_fingerprint` | Byte length and deterministic FNV-1a fingerprint of the last observed canonical body |
| `body_fingerprint` | Byte length and fingerprint of the recovery payload |
| `created_unix_ms` | Local creation time for display/order only |
| `body` | Complete Markdown body, excluding storage-owned YAML metadata |

The payload fingerprint is validated before preview or restore. Unknown/newer
recovery versions are not applied. A record is listed only when its body differs
from the current canonical body. Restore starts a new dirty revision and undo
epoch; discard removes only the selected record; save-copy atomically replaces
the selected destination. An acknowledged canonical save compacts a fulfilled
record.

Canonical save and recovery requests carry the project generation and document
revision. Completion changes visible state only when both still match. Project
workers are serial, so a checked older request cannot finish after a newer
request for that project.
