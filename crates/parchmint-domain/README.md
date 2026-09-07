# `parchmint-domain`

**Purpose:** Define projects and validate changes in plain Rust. Stable IDs
identify projects, nodes, documents, styles, metadata, dictionaries, comments,
and operations independently of titles, paths, or positions.

## Interface

`apply_project_command` returns a validated project, an inverse command, and
changed resource IDs. `synchronize_content_title` keeps display and content
titles aligned; `count_words` supplies the shared word-count rule.

`encode_stable_id` and `decode_stable_id` use 32 hexadecimal digits. Decoding
accepts either ASCII case and rejects malformed text without panicking; callers
map failures to their own error types. See [lib.rs](src/lib.rs) and [ids.rs](src/ids.rs).

## Project invariants

Groups contain groups or documents; documents are leaves; fixed roots stay in
place. Typed IDs prevent substituting one kind of resource for another.
`DeletionTombstone` retains a deleted node's type, former parent and order, and
restoration data.

Commands check the expected revision, validate the current tree, mutate a draft,
and validate that draft before publishing it. Failure leaves the input unchanged.
The inverse is a `RestoreState` snapshot of the previous project, retained by
application undo. The tree stores ID lookup separately from ordered child lists.

Errors distinguish invalid input, stale revisions, missing resources, duplicate
IDs, invalid trees, and cyclic moves. They can include IDs and field names, but
exclude authored prose. File access and external services belong to other crates.
