use std::collections::VecDeque;

use crate::{
    CanonicalAnchor, CanonicalComment, CanonicalProjection, DocumentId, EditorRevision,
    SemanticBlockSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Projection {
    pub(super) document_id: DocumentId,
    pub(super) revision: EditorRevision,
    pub(super) blocks: Vec<SemanticBlockSnapshot>,
    pub(super) comments: Vec<CanonicalComment>,
    pub(super) anchors: Vec<CanonicalAnchor>,
}

impl Projection {
    pub(super) fn canonical(self) -> CanonicalProjection {
        let body = self
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<String>();
        let word_count = body.split_whitespace().count();
        CanonicalProjection::new(
            self.document_id,
            self.revision,
            body,
            self.comments,
            self.anchors,
            word_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProjectionBatch {
    Incremental(Projection),
    FullSnapshot(Projection),
}

impl ProjectionBatch {
    #[cfg(test)]
    pub(super) const fn revision(&self) -> EditorRevision {
        match self {
            Self::Incremental(projection) | Self::FullSnapshot(projection) => projection.revision,
        }
    }
}

pub(super) struct ProjectionQueue {
    capacity: usize,
    pending: VecDeque<ProjectionBatch>,
    overflowed: bool,
}

impl ProjectionQueue {
    pub(super) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "projection queue capacity must be nonzero");
        Self {
            capacity,
            pending: VecDeque::new(),
            overflowed: false,
        }
    }

    pub(super) fn offer(&mut self, projection: Projection) {
        if let Some(ProjectionBatch::Incremental(previous)) = self.pending.back_mut()
            && previous.revision.next() == projection.revision
        {
            *previous = projection;
            return;
        }

        if self.pending.len() == self.capacity {
            self.pending.clear();
            self.overflowed = true;
        }
        self.pending
            .push_back(ProjectionBatch::Incremental(projection));
    }

    pub(super) fn take(&mut self) -> Option<ProjectionBatch> {
        if self.overflowed {
            let newest = self.pending.pop_back()?;
            self.pending.clear();
            self.overflowed = false;
            return Some(match newest {
                ProjectionBatch::Incremental(projection)
                | ProjectionBatch::FullSnapshot(projection) => {
                    ProjectionBatch::FullSnapshot(projection)
                }
            });
        }
        self.pending.pop_front()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }
}
