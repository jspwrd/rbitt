mod caching_manager;
mod coalescer;
mod error;
mod io_queue;
mod manager;
mod storage;
mod worker;

pub use caching_manager::{CachingDiskManager, WriteResult};
pub use coalescer::{FlushRequest, WriteCoalescer, WriteRegion, coalesce_blocks};
pub use error::DiskError;
pub use io_queue::{IoQueue, WriteOp, WritePriority};
pub use manager::{DiskManager, TorrentStorage};
pub use storage::{AllocationMode, FileEntry, PieceInfo};
pub use worker::IoWorker;

#[cfg(test)]
mod tests;
