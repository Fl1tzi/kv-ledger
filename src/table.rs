use alloc::vec::Vec;

use crate::ids::BlockId;

#[derive(Debug)]
pub(crate) struct Sequence {
    /// Block table: logical block index -> physical block.
    pub(crate) blocks: Vec<BlockId>,
    /// Number of tokens currently stored in this sequence.
    pub(crate) length: usize,
}
