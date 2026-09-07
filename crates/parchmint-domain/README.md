# `parchmint-domain`

This crate defines the meaning of a ParchMint project in plain Rust.

It defines stable IDs and the data structures for projects, groups, documents,
styles, metadata, and dictionaries. It also checks the project tree, keeps a
document's display title and content title in sync, and counts words. Other
crates read files and call external services.

The crate gives every caller the same rules for valid project state. Groups hold
groups or documents, documents are leaves, and fixed roots stay in place. Stable
IDs define identity; titles, paths, and positions do not.

## How it works

Domain changes are all-or-nothing. A rejected change leaves the input unchanged.

```text
current project + command
        |
        v
check revision -> validate current project -> apply rules to draft -> validate draft
        |                                                                   |
      error                                                new project + inverse
```

The returned `inverse` is a `RestoreState` snapshot of the complete prior
project state. The application crate stores that value in the project undo
list.

## Interface

`apply_project_command` returns a validated project, its inverse command, and
changed resource IDs. `synchronize_content_title` and `count_words` provide
shared title and word-count rules.

`encode_stable_id` and `decode_stable_id` own the shared 32-digit hexadecimal
ID representation. Decoding accepts either ASCII case and rejects malformed
identifier text without panicking; callers supply their own error types.

See [the source](src/lib.rs) for method signatures.

Each `DeletionTombstone` records the deleted node, its former parent and order,
its type, and the information needed to restore it. Styles, metadata fields,
comments, blocks, checkpoints, views, and project operations each have their
own ID type; groups use node IDs. The compiler rejects code that uses one kind
of ID where another is required.

## Implementation

The tree stores ID lookup separately from each node's ordered child list. A
mutation edits a draft and publishes it only after validation.

The crate returns different error variants for invalid input, an outdated
revision, a missing item, a duplicate ID, an invalid tree, or a move that
would create a cycle. Errors can include IDs and field names. They do not
include the writer's prose.
