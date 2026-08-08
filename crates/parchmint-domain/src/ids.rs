use std::fmt;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 16]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}(", stringify!($name))?;
                for byte in self.0 {
                    write!(formatter, "{byte:02x}")?;
                }
                formatter.write_str(")")
            }
        }
    };
}

stable_id!(ProjectId);
stable_id!(NodeId);
stable_id!(DocumentId);
stable_id!(StyleId);
stable_id!(MetadataFieldId);
stable_id!(BlockId);
stable_id!(CommentId);
stable_id!(CheckpointId);
stable_id!(ViewId);
stable_id!(ProjectOperationId);

impl NodeId {
    const MANUSCRIPT_ROOT_BYTES: [u8; 16] = [
        0x50, 0x41, 0x52, 0x43, 0x48, 0x4d, 0x49, 0x4e, 0x54, 0, 0, 0, 0, 0, 0, 1,
    ];
    const RESEARCH_ROOT_BYTES: [u8; 16] = [
        0x50, 0x41, 0x52, 0x43, 0x48, 0x4d, 0x49, 0x4e, 0x54, 0, 0, 0, 0, 0, 0, 2,
    ];

    pub const fn manuscript_root() -> Self {
        Self(Self::MANUSCRIPT_ROOT_BYTES)
    }

    pub const fn research_root() -> Self {
        Self(Self::RESEARCH_ROOT_BYTES)
    }

    pub fn is_fixed_root(self) -> bool {
        self.0 == Self::MANUSCRIPT_ROOT_BYTES || self.0 == Self::RESEARCH_ROOT_BYTES
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectRevision(u64);

impl ProjectRevision {
    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for ProjectRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
