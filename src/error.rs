use core::fmt;

use crate::ids::SeqId;

#[derive(Debug, PartialEq, Eq)]
pub enum BlockError {
    /// The free pool cannot satisfy the request. Reports how many blocks were
    /// `needed` versus how many were `free`, so the scheduler can react.
    OutOfBlocks { needed: usize, free: usize },
    /// The given sequence is not known to this manager.
    UnknownSeq(SeqId),
    /// A sequence with this id already exists.
    DuplicateSeq(SeqId),
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::OutOfBlocks { needed, free } => {
                write!(f, "out of blocks: needed {needed}, only {free} free")
            }
            BlockError::UnknownSeq(SeqId(id)) => write!(f, "unknown sequence {id}"),
            BlockError::DuplicateSeq(SeqId(id)) => write!(f, "duplicate sequence {id}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BlockError {}
