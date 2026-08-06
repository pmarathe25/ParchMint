# Deletion and Recently Deleted

- **DEL-001:** v1 has no Trash node in the live hierarchy.
- **DEL-002:** Delete removes content from current project state but preserves it in History.
- **DEL-003:** A deletion tombstone records stable node ID, title, type, section, former parent/order, deletion time, and restoring checkpoint.
- **DEL-004:** Recently Deleted lists deleted documents/groups, shows one formatted read-only preview, presents restore location, and can restore complete subtrees.
- **DEL-005:** Restore returns to the old location where possible or the relevant section root when the former parent is gone.
- **DEL-006:** Session project undo immediately reverses deletion while its entry remains available.
- **DEL-007:** v1 provides no purge, Empty Trash, or permanent history-erasure command.
