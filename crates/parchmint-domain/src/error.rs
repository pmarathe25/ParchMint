use std::{error::Error, fmt};

use crate::{NodeId, ProjectRevision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    StaleRevision {
        expected: ProjectRevision,
        actual: ProjectRevision,
    },
    MissingNode {
        id: NodeId,
    },
    DuplicateId {
        field: &'static str,
    },
    InvalidTree {
        reason: &'static str,
    },
    CycleDetected {
        node: NodeId,
        parent: NodeId,
    },
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    MissingItem {
        kind: &'static str,
    },
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "stale project revision: expected {}, current {}",
                expected.value(),
                actual.value()
            ),
            Self::MissingNode { id } => write!(formatter, "missing project node {id:?}"),
            Self::DuplicateId { field } => write!(formatter, "duplicate ID in {field}"),
            Self::InvalidTree { reason } => write!(formatter, "invalid project tree: {reason}"),
            Self::CycleDetected { node, parent } => {
                write!(
                    formatter,
                    "moving {node:?} below {parent:?} would create a cycle"
                )
            }
            Self::InvalidInput { field, reason } => {
                write!(formatter, "invalid {field}: {reason}")
            }
            Self::MissingItem { kind } => write!(formatter, "missing {kind}"),
        }
    }
}

impl Error for DomainError {}
