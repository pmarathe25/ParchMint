# `parchmint-domain`

## What it does

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
check revision -> apply rules -> check full project
        |                            |
      error                    new project + inverse
```

The returned `inverse` describes how to reverse the complete change. The
application crate stores that value in the project undo list.

## Public API

Callers use this external surface:

```rust
pub struct ProjectId([u8; 16]);
pub struct NodeId([u8; 16]);
pub struct DocumentId([u8; 16]);
pub struct ProjectRevision(u64);

impl ProjectId {
    pub fn from_bytes(bytes: [u8; 16]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 16];
}

pub enum NodeKind {
    Root(ProjectSection),
    Group,
    Document(DocumentId),
}

pub struct Project {
    pub id: ProjectId,
    pub revision: ProjectRevision,
    pub nodes: OrderedTree<NodeId, ProjectNode>,
    pub styles: StyleCatalog,
    pub metadata: MetadataCatalog,
    pub dictionary: ProjectDictionary,
    pub export_settings: ProjectExportSettings,
    pub deleted: BTreeMap<NodeId, DeletionTombstone>,
}

pub fn apply_project_command(
    project: &Project,
    expected: ProjectRevision,
    command: ProjectCommand,
) -> Result<AppliedProjectCommand, DomainError>;

pub struct AppliedProjectCommand {
    pub project: Project,
    pub inverse: ProjectCommand,
    pub changed_resources: ResourceSet,
}

pub fn synchronize_content_title(
    display_title: &str,
    previous_content_title: Option<&str>,
    new_content_title: Option<&str>,
) -> TitleChange;

pub fn count_words<'a>(
    blocks: impl Iterator<Item = SemanticBlockRef<'a>>,
) -> WordCount;
```

Each `DeletionTombstone` records the deleted node, its former parent and order,
its type, and the information needed to restore it. Groups, styles, metadata
fields, comments, blocks, checkpoints, views, project operations, and undo
entries also have their own ID types. The compiler rejects code that uses one
kind of ID where another is required.

## Implementation

The tree stores ID lookup separately from each node's ordered child list. A
mutation edits a draft and publishes it only after validation:

```rust
fn apply(project: &Project, command: ProjectCommand) -> Result<Applied, DomainError> {
    check_expected_revision(project, command.expected_revision())?;
    let mut draft = project.clone();
    let inverse = command.apply_to(&mut draft)?;
    validate_roots(&draft)?;
    validate_parents_and_cycles(&draft)?;
    validate_ids_and_order(&draft)?;
    draft.revision = draft.revision.next();
    Ok(Applied::new(draft, inverse))
}
```

The crate returns different error variants for invalid input, an outdated
revision, a missing item, a duplicate ID, or an invalid tree. Errors can include
IDs and field names. They do not include the writer's prose.
