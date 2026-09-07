use std::fmt;

/// The shared text representation of a stable ID: 32 lowercase hexadecimal digits.
pub fn encode_stable_id(bytes: &[u8; 16]) -> String {
    use fmt::Write as _;

    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

/// Reads a stable ID without slicing UTF-8. Uppercase hexadecimal is accepted.
/// Callers translate invalid input into their own boundary's error type.
pub fn decode_stable_id(value: &str) -> Option<[u8; 16]> {
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    if value.len() != 32 {
        return None;
    }
    let mut decoded = [0; 16];
    for (byte, pair) in decoded.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *byte = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(decoded)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_use_fixed_width_hex_and_accept_either_case() {
        let bytes = [
            0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76,
            0x54, 0x32,
        ];
        let encoded = "000123456789abcdeffedcba98765432";
        assert_eq!(encode_stable_id(&bytes), encoded);
        assert_eq!(decode_stable_id(encoded), Some(bytes));
        assert_eq!(decode_stable_id(&encoded.to_uppercase()), Some(bytes));
    }

    #[test]
    fn stable_ids_reject_malformed_text_without_panicking() {
        for value in [
            String::new(),
            "0".repeat(31),
            "0".repeat(33),
            "g0".repeat(16),
            "+a".repeat(16),
            " 0".repeat(16),
            format!("a\u{20ac}{}", "0".repeat(28)),
            "\u{1f600}".repeat(8),
        ] {
            assert_eq!(decode_stable_id(&value), None, "{value:?}");
        }
    }
}
