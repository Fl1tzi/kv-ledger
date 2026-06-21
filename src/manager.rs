//! Orchestrates the free pool and the block table of each sequence.
//!
//! This is the actual bookkeeping.
//! Admit a new sequence (therefore reserving physical blocks) and extend
//! it by new tokens. At each block boundary a new block is allocated.
//! The block is freed back to the pool once it is not needed anymore.

use alloc::vec::Vec;

use crate::SeqMap;
use crate::error::BlockError;
use crate::ids::{BlockId, SeqId};
use crate::table::Sequence;

/// # The logical layer for one physical slab
///
/// This is pure bookkeeping. The sequences are managed and
/// stored in the sequence map ([`SeqMap`]). All the other free
/// blocks are tracked as a LIFO stack. Recently freed blocks are
/// reused first.
// TODO: Is LIFO the most performant variant? LIFO was initially chosen
// because it could be cache friendly.
//
// TODO: Add a `FreePool` trait to add more options like copy-on-write.
pub struct BlockManager {
    /// `B`: tokens per block.
    ///
    /// The optimal size depends on the memory and sequence length.
    /// Smaller blocks reduce fragmentation, larger blocks improve access.
    block_size: usize,
    /// `N`: total physical blocks in the slab.
    num_blocks: usize,
    /// Free pool
    free: Vec<BlockId>,
    /// Active sequences
    seqs: SeqMap,
}

impl BlockManager {
    /// Create a manager owning `num_blocks` block that each hold `block_size`
    /// tokens.
    ///
    /// # Panics
    ///
    /// - `block_size == 0`
    /// - `num_blocks` exceeds [`BlockId::MAX`]
    pub fn new(num_blocks: usize, block_size: usize) -> Self {
        assert!(block_size > 0, "block_size must be greater-than 0");
        assert!(
            num_blocks <= BlockId::MAX as usize,
            "num blocks is greater-than the number of block ids"
        );
        // Fill the pool with the number of blocks.
        // We fill in reverse order so that the lowest ids gets popped first.
        let free = (0..num_blocks as u32).rev().map(BlockId).collect();
        Self {
            block_size,
            num_blocks,
            free,
            seqs: SeqMap::default(),
        }
    }

    /// Returns true if a new sequence of `prompt_len` tokens
    /// can be admitted. False if the there are not enough free blocks.
    pub fn can_allocate(&self, prompt_len: usize) -> bool {
        self.blocks_needed(prompt_len) <= self.free.len()
    }

    /// Admit a new sequence to reserve blocks.
    pub fn allocate(&mut self, seq: SeqId, prompt_len: usize) -> Result<(), BlockError> {
        // check if the sequence is already in the pool
        if self.seqs.contains_key(&seq) {
            return Err(BlockError::DuplicateSeq(seq));
        }

        let needed = self.blocks_needed(prompt_len);
        if !self.can_allocate(prompt_len) {
            return Err(BlockError::OutOfBlocks {
                needed,
                free: self.free.len(),
            });
        }

        // insert the new blocks into the active sequences
        let mut blocks = Vec::with_capacity(needed);
        for _ in 0..needed {
            blocks.push(self.acquire().expect("free pool checked above"));
        }

        self.seqs.insert(
            seq,
            Sequence {
                blocks,
                length: prompt_len,
            },
        );
        #[cfg(test)]
        self.debug_check_invariants();
        Ok(())
    }

    /// Extend an exisiting sequence by one token.
    ///
    /// Returns `Ok(Some(block))` if a fresh block needs to be allocated.
    /// This needs to be handled by the engine.
    ///
    /// Returns `Ok(None)` if the block still has room.
    pub fn append_token(&mut self, seq: SeqId) -> Result<Option<BlockId>, BlockError> {
        // Retrieve the sequence from the map and check if a token would fit
        // in the last block.
        let needs_block = match self.seqs.get(&seq) {
            // empty or the last block is exactly full
            Some(s) => s.length % self.block_size == 0,
            None => return Err(BlockError::UnknownSeq(seq)),
        };

        let new_block = if needs_block {
            Some(
                self.acquire()
                    .ok_or(BlockError::OutOfBlocks { needed: 1, free: 0 })?,
            )
        } else {
            None
        };

        // push the block to the sequence
        let s = self.seqs.get_mut(&seq).expect("existence checked above");
        if let Some(block) = new_block {
            s.blocks.push(block);
        }
        s.length += 1; // only after any allocation succeeded
        #[cfg(test)]
        self.debug_check_invariants();
        Ok(new_block)
    }

    /// Release a sequence.
    ///
    /// All of the blocks corresponding to that sequence are returned to the free pool.
    pub fn free(&mut self, seq: SeqId) {
        if let Some(s) = self.seqs.remove(&seq) {
            for block in s.blocks {
                self.release(block);
            }
        }
        #[cfg(test)]
        self.debug_check_invariants();
    }

    /// The ordered layout of this sequence.
    pub fn block_table(&self, seq: SeqId) -> Option<&[BlockId]> {
        self.seqs.get(&seq).map(|s| s.blocks.as_slice())
    }

    /// Number of tokens currently stored in the sequence, or `None` if unknown.
    pub fn token_count(&self, seq: SeqId) -> Option<usize> {
        self.seqs.get(&seq).map(|s| s.length)
    }

    /// The number of free blocks.
    pub fn num_free(&self) -> usize {
        self.free.len()
    }

    /// The number of non-free blocks.
    pub fn num_blocks(&self) -> usize {
        self.num_blocks
    }

    /// Tokens per block (`B`).
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Blocks required to hold `len` tokens: `ceil(len / B)`.
    fn blocks_needed(&self, len: usize) -> usize {
        len.div_ceil(self.block_size)
    }

    /// Hand out a single free block or `None` if the pool is empty.
    fn acquire(&mut self) -> Option<BlockId> {
        self.free.pop()
    }

    /// Return one block to the pool.
    fn release(&mut self, block: BlockId) {
        self.free.push(block);
    }

    /// Self-check. Verifies that the blocks are correctly managed.
    #[cfg(test)]
    fn debug_check_invariants(&self) {
        let mut used = 0;
        for s in self.seqs.values() {
            // The sequence holds the number of blocks it needs
            // for the number of tokens.
            assert_eq!(s.blocks.len(), self.blocks_needed(s.length));
            used += s.blocks.len();
        }
        // The number of blocks is correct.
        assert_eq!(self.free.len() + used, self.num_blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::BlockManager;
    use crate::{BlockError, SeqId};

    fn mgr(num_blocks: usize, block_size: usize) -> BlockManager {
        BlockManager::new(num_blocks, block_size)
    }

    #[test]
    fn new_starts_all_free() {
        let m = mgr(8, 4);
        assert_eq!(m.num_free(), 8);
        assert_eq!(m.num_blocks(), 8);
        assert_eq!(m.block_size(), 4);
    }

    #[test]
    fn allocate_consumes_exact_block_count() {
        let mut m = mgr(8, 4);
        // 9 tokens, B=4 -> ceil(9/4) = 3 blocks.
        m.allocate(SeqId(1), 9).unwrap();
        assert_eq!(m.num_free(), 8 - 3);
        // Lowest ids first, in logical order.
        let table: Vec<u32> = m
            .block_table(SeqId(1))
            .unwrap()
            .iter()
            .map(|b| b.0)
            .collect();
        assert_eq!(table, vec![0, 1, 2]);
    }

    #[test]
    fn append_allocates_only_on_boundary() {
        let mut m = mgr(8, 2); // B = 2
        m.allocate(SeqId(1), 0).unwrap(); // empty: 0 blocks
        assert_eq!(m.block_table(SeqId(1)).unwrap().len(), 0);

        // length goes 0->1->2->3->4; a new block appears if length % 2 == 0.
        let got: Vec<bool> = (0..4)
            .map(|_| m.append_token(SeqId(1)).unwrap().is_some())
            .collect();
        assert_eq!(got, vec![true, false, true, false]);
        assert_eq!(m.block_table(SeqId(1)).unwrap().len(), 2); // 4 tokens / 2
    }

    #[test]
    fn oom_fires_at_exact_threshold() {
        let mut m = mgr(4, 4); // 4 blocks
        // 17 tokens need ceil(17/4) = 5 blocks > 4 free.
        assert_eq!(
            m.allocate(SeqId(1), 17),
            Err(BlockError::OutOfBlocks { needed: 5, free: 4 })
        );
        // Exactly 16 tokens = 4 blocks fits and drains the pool.
        assert!(m.allocate(SeqId(1), 16).is_ok());
        assert_eq!(m.num_free(), 0);
        assert!(!m.can_allocate(1));
        // seq1 length 16, 16 % 4 == 0 -> wants a block, but the pool is empty.
        assert_eq!(
            m.append_token(SeqId(1)),
            Err(BlockError::OutOfBlocks { needed: 1, free: 0 })
        );
    }

    #[test]
    fn free_returns_every_block() {
        let mut m = mgr(8, 4);
        m.allocate(SeqId(1), 9).unwrap(); // 3 blocks
        m.allocate(SeqId(2), 4).unwrap(); // 1 block
        m.free(SeqId(1));
        m.free(SeqId(2));
        assert_eq!(m.num_free(), 8);
        assert!(m.block_table(SeqId(1)).is_none());
    }

    #[test]
    fn full_pool_then_reuse() {
        let mut m = mgr(2, 4); // 2 blocks
        m.allocate(SeqId(1), 4).unwrap(); // 1 block
        m.allocate(SeqId(2), 4).unwrap(); // 1 block; pool now empty
        assert_eq!(m.num_free(), 0);
        assert_eq!(
            m.allocate(SeqId(3), 1),
            Err(BlockError::OutOfBlocks { needed: 1, free: 0 })
        );
        // Free one: the freed block is reused
        m.free(SeqId(1));
        assert_eq!(m.num_free(), 1);
        assert!(m.allocate(SeqId(3), 1).is_ok());
        assert_eq!(m.num_free(), 0);
    }

    #[test]
    fn duplicate_and_unknown_seq() {
        let mut m = mgr(8, 4);
        m.allocate(SeqId(1), 4).unwrap();
        assert_eq!(
            m.allocate(SeqId(1), 4),
            Err(BlockError::DuplicateSeq(SeqId(1)))
        );

        assert_eq!(
            m.append_token(SeqId(99)),
            Err(BlockError::UnknownSeq(SeqId(99)))
        );
        assert!(m.block_table(SeqId(99)).is_none());
        let before = m.num_free();
        m.free(SeqId(99)); // no-op
        assert_eq!(m.num_free(), before);
    }
}
