use super::*;
use crate::buffer_pool::{BLOCK_SIZE, BUFFER_POOL_BLOCKS, BUFFER_POOL_PIECES};
use crate::memory_budget::{BLOCK_CACHE_RATIO, MAX_CACHE_MEMORY, PIECE_CACHE_RATIO};
use bytes::Bytes;

// ========================
// BlockCache tests
// ========================

#[test]
fn test_block_cache_new() {
    let cache = BlockCache::new(1024 * 1024);
    assert_eq!(cache.memory_used(), 0);
    assert_eq!(cache.memory_limit(), 1024 * 1024);
    assert_eq!(cache.pieces_count(), 0);
}

#[test]
fn test_block_cache_add_single_block() {
    let cache = BlockCache::new(1024 * 1024);
    let data = Bytes::from(vec![0xAB; 16384]);

    // Piece of 16384 bytes = 1 block, should be complete immediately
    let complete = cache.add_block("hash1", 0, 0, data, 16384, 1);
    assert!(complete);
    assert_eq!(cache.memory_used(), 16384);
    assert!(cache.has_piece("hash1", 0));
    assert!(cache.is_piece_complete("hash1", 0));
}

#[test]
fn test_block_cache_add_multiple_blocks() {
    let cache = BlockCache::new(1024 * 1024);
    let piece_length = 16384 * 3; // 3 blocks

    // Add first block - not complete yet
    let complete = cache.add_block(
        "hash1",
        0,
        0,
        Bytes::from(vec![1u8; 16384]),
        piece_length,
        1,
    );
    assert!(!complete);
    assert!(!cache.is_piece_complete("hash1", 0));

    // Add second block
    let complete = cache.add_block(
        "hash1",
        0,
        16384,
        Bytes::from(vec![2u8; 16384]),
        piece_length,
        1,
    );
    assert!(!complete);

    // Add third block - now complete
    let complete = cache.add_block(
        "hash1",
        0,
        32768,
        Bytes::from(vec![3u8; 16384]),
        piece_length,
        1,
    );
    assert!(complete);
    assert!(cache.is_piece_complete("hash1", 0));
    assert_eq!(cache.memory_used(), piece_length as usize);
}

#[test]
fn test_block_cache_get_assembled_piece() {
    let cache = BlockCache::new(1024 * 1024);
    let piece_length = 16384 * 2;

    cache.add_block(
        "hash1",
        0,
        0,
        Bytes::from(vec![0xAA; 16384]),
        piece_length,
        1,
    );
    cache.add_block(
        "hash1",
        0,
        16384,
        Bytes::from(vec![0xBB; 16384]),
        piece_length,
        1,
    );

    let assembled = cache.get_assembled_piece("hash1", 0).unwrap();
    assert_eq!(assembled.len(), piece_length as usize);
    assert!(assembled[..16384].iter().all(|&b| b == 0xAA));
    assert!(assembled[16384..].iter().all(|&b| b == 0xBB));
}

#[test]
fn test_block_cache_get_assembled_piece_missing() {
    let cache = BlockCache::new(1024 * 1024);
    assert!(cache.get_assembled_piece("nonexistent", 0).is_none());
}

#[test]
fn test_block_cache_remove_piece() {
    let cache = BlockCache::new(1024 * 1024);
    cache.add_block("hash1", 0, 0, Bytes::from(vec![0u8; 16384]), 16384, 1);

    assert_eq!(cache.memory_used(), 16384);
    let removed = cache.remove_piece("hash1", 0);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().len(), 16384);
    assert_eq!(cache.memory_used(), 0);
    assert!(!cache.has_piece("hash1", 0));
}

#[test]
fn test_block_cache_remove_nonexistent() {
    let cache = BlockCache::new(1024 * 1024);
    assert!(cache.remove_piece("nonexistent", 0).is_none());
}

#[test]
fn test_block_cache_coalesced_regions_contiguous() {
    let cache = BlockCache::new(1024 * 1024);
    let piece_length = 16384 * 3;

    // Add contiguous blocks in order
    cache.add_block(
        "hash1",
        0,
        0,
        Bytes::from(vec![1u8; 16384]),
        piece_length,
        1,
    );
    cache.add_block(
        "hash1",
        0,
        16384,
        Bytes::from(vec![2u8; 16384]),
        piece_length,
        1,
    );
    cache.add_block(
        "hash1",
        0,
        32768,
        Bytes::from(vec![3u8; 16384]),
        piece_length,
        1,
    );

    let regions = cache.get_coalesced_regions("hash1", 0);
    // All contiguous, should be one region
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].offset, 0);
    assert_eq!(regions[0].data.len(), piece_length as usize);
}

#[test]
fn test_block_cache_coalesced_regions_gap() {
    let cache = BlockCache::new(1024 * 1024);
    let piece_length = 16384 * 3;

    // Add blocks with gap (missing middle block)
    cache.add_block(
        "hash1",
        0,
        0,
        Bytes::from(vec![1u8; 16384]),
        piece_length,
        1,
    );
    cache.add_block(
        "hash1",
        0,
        32768,
        Bytes::from(vec![3u8; 16384]),
        piece_length,
        1,
    );

    let regions = cache.get_coalesced_regions("hash1", 0);
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].offset, 0);
    assert_eq!(regions[0].data.len(), 16384);
    assert_eq!(regions[1].offset, 32768);
    assert_eq!(regions[1].data.len(), 16384);
}

#[test]
fn test_block_cache_coalesced_regions_empty() {
    let cache = BlockCache::new(1024 * 1024);
    let regions = cache.get_coalesced_regions("nonexistent", 0);
    assert!(regions.is_empty());
}

#[test]
fn test_block_cache_finalize_and_verify_v1() {
    use sha1::{Digest, Sha1};

    let cache = BlockCache::new(1024 * 1024);
    let data = vec![0xABu8; 16384];

    // Compute expected hash
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let expected_hash = hasher.finalize().to_vec();

    cache.add_block("hash1", 0, 0, Bytes::from(data), 16384, 1);

    assert!(cache.finalize_and_verify("hash1", 0, &expected_hash));
}

#[test]
fn test_block_cache_finalize_and_verify_v2() {
    use sha2::{Digest, Sha256};

    let cache = BlockCache::new(1024 * 1024);
    let data = vec![0xCDu8; 16384];

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let expected_hash = hasher.finalize().to_vec();

    cache.add_block("hash1", 0, 0, Bytes::from(data), 16384, 2);

    assert!(cache.finalize_and_verify("hash1", 0, &expected_hash));
}

#[test]
fn test_block_cache_finalize_wrong_hash() {
    let cache = BlockCache::new(1024 * 1024);
    cache.add_block("hash1", 0, 0, Bytes::from(vec![0xAB; 16384]), 16384, 1);

    let wrong_hash = vec![0u8; 20];
    assert!(!cache.finalize_and_verify("hash1", 0, &wrong_hash));
}

#[test]
fn test_block_cache_finalize_nonexistent() {
    let cache = BlockCache::new(1024 * 1024);
    assert!(!cache.finalize_and_verify("nonexistent", 0, &[0u8; 20]));
}

#[test]
fn test_block_cache_finalize_multiblock_v1() {
    use sha1::{Digest, Sha1};

    let cache = BlockCache::new(1024 * 1024);
    let piece_length = 16384 * 2;
    let block1 = vec![0xAAu8; 16384];
    let block2 = vec![0xBBu8; 16384];

    let mut hasher = Sha1::new();
    hasher.update(&block1);
    hasher.update(&block2);
    let expected_hash = hasher.finalize().to_vec();

    // Add blocks in order so streaming hash can advance
    cache.add_block("hash1", 0, 0, Bytes::from(block1), piece_length, 1);
    cache.add_block("hash1", 0, 16384, Bytes::from(block2), piece_length, 1);

    assert!(cache.finalize_and_verify("hash1", 0, &expected_hash));
}

#[test]
fn test_block_cache_is_under_pressure() {
    let cache = BlockCache::new(100); // Very small limit

    // Not under pressure at start
    assert!(!cache.is_under_pressure());

    // Add enough data to exceed 90%
    cache.add_block("hash1", 0, 0, Bytes::from(vec![0u8; 91]), 91, 1);
    assert!(cache.is_under_pressure());
}

#[test]
fn test_block_cache_multiple_pieces() {
    let cache = BlockCache::new(1024 * 1024);

    cache.add_block("hash1", 0, 0, Bytes::from(vec![1u8; 16384]), 16384, 1);
    cache.add_block("hash1", 1, 0, Bytes::from(vec![2u8; 16384]), 16384, 1);
    cache.add_block("hash2", 0, 0, Bytes::from(vec![3u8; 16384]), 16384, 1);

    assert_eq!(cache.pieces_count(), 3);
    assert!(cache.has_piece("hash1", 0));
    assert!(cache.has_piece("hash1", 1));
    assert!(cache.has_piece("hash2", 0));
    assert!(!cache.has_piece("hash2", 1));
}

#[test]
fn test_block_cache_clear() {
    let cache = BlockCache::new(1024 * 1024);
    cache.add_block("hash1", 0, 0, Bytes::from(vec![0u8; 16384]), 16384, 1);
    cache.add_block("hash1", 1, 0, Bytes::from(vec![0u8; 16384]), 16384, 1);

    assert_eq!(cache.pieces_count(), 2);
    assert!(cache.memory_used() > 0);

    cache.clear();
    assert_eq!(cache.pieces_count(), 0);
    assert_eq!(cache.memory_used(), 0);
}

#[test]
fn test_block_cache_duplicate_block() {
    let cache = BlockCache::new(1024 * 1024);

    cache.add_block("hash1", 0, 0, Bytes::from(vec![1u8; 16384]), 16384, 1);
    let mem_after_first = cache.memory_used();

    // Re-add same block offset - should not double count memory
    cache.add_block("hash1", 0, 0, Bytes::from(vec![2u8; 16384]), 16384, 1);
    assert_eq!(cache.memory_used(), mem_after_first);
}

// ========================
// HashState tests
// ========================

#[test]
fn test_hash_state_v1() {
    use sha1::{Digest, Sha1};

    let mut state = HashState::new_v1();
    state.update(b"hello world");
    let result = state.finalize();

    let mut expected = Sha1::new();
    expected.update(b"hello world");
    assert_eq!(result, expected.finalize().to_vec());
}

#[test]
fn test_hash_state_v2() {
    use sha2::{Digest, Sha256};

    let mut state = HashState::new_v2();
    state.update(b"hello world");
    let result = state.finalize();

    let mut expected = Sha256::new();
    expected.update(b"hello world");
    assert_eq!(result, expected.finalize().to_vec());
}

// ========================
// PieceCache tests (ARC algorithm)
// ========================

#[test]
fn test_piece_cache_new() {
    let cache = PieceCache::new(10);
    assert_eq!(cache.capacity(), 10);
    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());
    assert_eq!(cache.memory_used(), 0);
}

#[test]
fn test_piece_cache_insert_and_get() {
    let cache = PieceCache::new(10);
    let data = Bytes::from(vec![0xAB; 1024]);

    cache.insert("hash1", 0, data.clone(), true);
    assert_eq!(cache.len(), 1);
    assert!(!cache.is_empty());
    assert!(cache.contains("hash1", 0));

    let retrieved = cache.get("hash1", 0).unwrap();
    assert_eq!(retrieved, data);
}

#[test]
fn test_piece_cache_get_nonexistent() {
    let cache = PieceCache::new(10);
    assert!(cache.get("hash1", 0).is_none());
    assert!(!cache.contains("hash1", 0));
}

#[test]
fn test_piece_cache_remove() {
    let cache = PieceCache::new(10);
    cache.insert("hash1", 0, Bytes::from(vec![0u8; 1024]), true);

    let removed = cache.remove("hash1", 0);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().len(), 1024);
    assert!(!cache.contains("hash1", 0));
    assert_eq!(cache.len(), 0);
}

#[test]
fn test_piece_cache_remove_nonexistent() {
    let cache = PieceCache::new(10);
    assert!(cache.remove("hash1", 0).is_none());
}

#[test]
fn test_piece_cache_memory_tracking() {
    let cache = PieceCache::new(10);

    cache.insert("hash1", 0, Bytes::from(vec![0u8; 1000]), true);
    assert_eq!(cache.memory_used(), 1000);

    cache.insert("hash1", 1, Bytes::from(vec![0u8; 2000]), true);
    assert_eq!(cache.memory_used(), 3000);

    cache.remove("hash1", 0);
    assert_eq!(cache.memory_used(), 2000);
}

#[test]
fn test_piece_cache_eviction_at_capacity() {
    let cache = PieceCache::new(3);

    // Fill to capacity
    cache.insert("hash1", 0, Bytes::from(vec![1u8; 100]), true);
    cache.insert("hash1", 1, Bytes::from(vec![2u8; 100]), true);
    cache.insert("hash1", 2, Bytes::from(vec![3u8; 100]), true);
    assert_eq!(cache.len(), 3);

    // Add one more - should evict oldest
    cache.insert("hash1", 3, Bytes::from(vec![4u8; 100]), true);
    assert!(cache.len() <= 3);

    // Most recent should still be there
    assert!(cache.contains("hash1", 3));
}

#[test]
fn test_piece_cache_promotion_on_hit() {
    let cache = PieceCache::new(3);

    // Insert three items into T1
    cache.insert("hash1", 0, Bytes::from(vec![1u8; 100]), true);
    cache.insert("hash1", 1, Bytes::from(vec![2u8; 100]), true);
    cache.insert("hash1", 2, Bytes::from(vec![3u8; 100]), true);

    // Access item 0 - should promote from T1 to T2
    let _ = cache.get("hash1", 0);

    // Add new item - should evict from T1 (not item 0 which is now in T2)
    cache.insert("hash1", 3, Bytes::from(vec![4u8; 100]), true);

    // Item 0 should survive (it was promoted to T2)
    assert!(cache.contains("hash1", 0));
}

#[test]
fn test_piece_cache_clear() {
    let cache = PieceCache::new(10);
    cache.insert("hash1", 0, Bytes::from(vec![0u8; 1024]), true);
    cache.insert("hash1", 1, Bytes::from(vec![0u8; 1024]), true);

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.memory_used(), 0);
    assert!(cache.is_empty());
}

#[test]
fn test_piece_cache_multiple_torrents() {
    let cache = PieceCache::new(10);

    cache.insert("hash_a", 0, Bytes::from(vec![1u8; 100]), true);
    cache.insert("hash_b", 0, Bytes::from(vec![2u8; 200]), true);
    cache.insert("hash_a", 1, Bytes::from(vec![3u8; 300]), true);

    assert_eq!(cache.len(), 3);
    assert!(cache.contains("hash_a", 0));
    assert!(cache.contains("hash_b", 0));
    assert!(cache.contains("hash_a", 1));
    assert!(!cache.contains("hash_b", 1));
}

// ========================
// BufferPool tests
// ========================

#[test]
fn test_buffer_pool_new() {
    let pool = BufferPool::new();
    assert_eq!(pool.block_buffers_available(), BUFFER_POOL_BLOCKS);
    assert_eq!(pool.piece_buffers_available(), BUFFER_POOL_PIECES);
}

#[test]
fn test_buffer_pool_get_return_block() {
    let pool = BufferPool::new();
    let initial = pool.block_buffers_available();

    let buf = pool.get_block_buffer();
    assert!(buf.capacity() >= BLOCK_SIZE);
    assert_eq!(pool.block_buffers_available(), initial - 1);

    pool.return_block_buffer(buf);
    assert_eq!(pool.block_buffers_available(), initial);
}

#[test]
fn test_buffer_pool_get_return_piece() {
    let pool = BufferPool::new();
    let initial = pool.piece_buffers_available();

    let buf = pool.get_piece_buffer(1024 * 1024);
    assert!(buf.capacity() >= 1024 * 1024);
    assert_eq!(pool.piece_buffers_available(), initial - 1);

    pool.return_piece_buffer(buf);
    assert_eq!(pool.piece_buffers_available(), initial);
}

#[test]
fn test_buffer_pool_exhaustion() {
    let pool = BufferPool::new();

    // Drain all block buffers
    let mut bufs = Vec::new();
    for _ in 0..BUFFER_POOL_BLOCKS {
        bufs.push(pool.get_block_buffer());
    }
    assert_eq!(pool.block_buffers_available(), 0);

    // Should still work - allocates new buffer
    let extra = pool.get_block_buffer();
    assert!(extra.capacity() >= BLOCK_SIZE);

    // Return all buffers
    for buf in bufs {
        pool.return_block_buffer(buf);
    }
    assert_eq!(pool.block_buffers_available(), BUFFER_POOL_BLOCKS);
}

#[test]
fn test_buffer_pool_piece_small_request() {
    let pool = BufferPool::new();

    // Request smaller than default piece size - should reuse pooled buffer
    let buf = pool.get_piece_buffer(1024);
    assert!(buf.capacity() >= 1024);
}

#[test]
fn test_buffer_pool_default() {
    let pool = BufferPool::default();
    // Default creates empty pools (no pre-allocation)
    assert_eq!(pool.block_buffers_available(), 0);
    assert_eq!(pool.piece_buffers_available(), 0);

    // Should still work via fallback allocation
    let buf = pool.get_block_buffer();
    assert!(buf.capacity() >= BLOCK_SIZE);
}

// ========================
// MemoryBudget tests
// ========================

#[test]
fn test_memory_budget_new() {
    let budget = MemoryBudget::new(100 * 1024 * 1024);
    assert_eq!(budget.total_limit(), 100 * 1024 * 1024);
    assert_eq!(budget.current_usage(), 0);
    assert!(!budget.is_under_pressure());
}

#[test]
fn test_memory_budget_capped_at_max() {
    let budget = MemoryBudget::new(usize::MAX);
    assert_eq!(budget.total_limit(), MAX_CACHE_MEMORY);
}

#[test]
fn test_memory_budget_ratios() {
    let total = 100 * 1024 * 1024;
    let budget = MemoryBudget::new(total);
    assert_eq!(
        budget.block_cache_limit(),
        (total as f32 * BLOCK_CACHE_RATIO) as usize
    );
    assert_eq!(
        budget.piece_cache_limit(),
        (total as f32 * PIECE_CACHE_RATIO) as usize
    );
}

#[test]
fn test_memory_budget_try_allocate_success() {
    let budget = MemoryBudget::new(1024);

    let permit = budget.try_allocate(512);
    assert!(permit.is_some());
    assert_eq!(budget.current_usage(), 512);
    assert_eq!(permit.unwrap().bytes(), 512);
}

#[test]
fn test_memory_budget_try_allocate_failure() {
    let budget = MemoryBudget::new(1024);

    // Try to allocate more than limit
    let permit = budget.try_allocate(2048);
    assert!(permit.is_none());
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_memory_budget_permit_drop_releases() {
    let budget = MemoryBudget::new(1024);

    {
        let _permit = budget.try_allocate(512).unwrap();
        assert_eq!(budget.current_usage(), 512);
    }
    // Permit dropped, memory should be released
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_memory_budget_multiple_permits() {
    let budget = MemoryBudget::new(1024);

    let p1 = budget.try_allocate(256).unwrap();
    let p2 = budget.try_allocate(256).unwrap();
    assert_eq!(budget.current_usage(), 512);

    drop(p1);
    assert_eq!(budget.current_usage(), 256);

    drop(p2);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_memory_budget_permit_resize() {
    let budget = MemoryBudget::new(1024);
    let mut permit = budget.try_allocate(256).unwrap();
    assert_eq!(budget.current_usage(), 256);

    // Grow
    permit.resize(512);
    assert_eq!(budget.current_usage(), 512);
    assert_eq!(permit.bytes(), 512);

    // Shrink
    permit.resize(128);
    assert_eq!(budget.current_usage(), 128);
    assert_eq!(permit.bytes(), 128);
}

#[test]
fn test_memory_budget_is_under_pressure() {
    let budget = MemoryBudget::new(100);

    let _p1 = budget.try_allocate(89);
    assert!(!budget.is_under_pressure());

    let _p2 = budget.try_allocate(2);
    assert!(budget.is_under_pressure());
}

#[test]
fn test_memory_budget_allocate_exactly_at_limit() {
    let budget = MemoryBudget::new(1024);

    let permit = budget.try_allocate(1024);
    assert!(permit.is_some());
    assert_eq!(budget.current_usage(), 1024);

    // Can't allocate any more
    let permit2 = budget.try_allocate(1);
    assert!(permit2.is_none());
}
