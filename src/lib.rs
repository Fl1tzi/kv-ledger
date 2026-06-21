//! Block management for paged attention.
//!
//! Call [`BlockManager::allocate`] when admitting a request,
//! [`BlockManager::append_token`] each decode step, and [`BlockManager::free`]
//! when the request completes.
//!
//! ```rust
//! use kv_ledger::{BlockManager, SeqId};
//!
//! let mut mgr = BlockManager::new(8, 16); // 8 blocks, 16 tokens each
//!
//! mgr.allocate(SeqId(1), 20).unwrap(); // admit a 20-token prompt
//! mgr.append_token(SeqId(1)).unwrap(); // decode one token
//! let _table = mgr.block_table(SeqId(1)); // pass to the attention kernel
//! mgr.free(SeqId(1)); // request done, blocks reclaimed
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod error;
mod ids;
mod manager;
mod table;

pub use error::BlockError;
pub use ids::{BlockId, SeqId};
pub use manager::BlockManager;

/// The per-sequence map.
///
/// - `std`: `HashMap`
/// - `no_std`: `BTreeMap`
#[cfg(feature = "std")]
pub(crate) type SeqMap = std::collections::HashMap<SeqId, table::Sequence>;
#[cfg(not(feature = "std"))]
pub(crate) type SeqMap = alloc::collections::BTreeMap<SeqId, table::Sequence>;
