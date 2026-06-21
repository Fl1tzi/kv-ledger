/// Index of a physical block in the global slab, in the range
/// `[0, num_blocks)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub u32);

impl BlockId {
    pub const MAX: u32 = u32::MAX;
}

/// Identifier for an active sequence.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct SeqId(pub u64);
