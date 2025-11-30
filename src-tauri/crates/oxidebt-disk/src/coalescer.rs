use bytes::Bytes;
use std::time::Duration;
use tokio::sync::mpsc;

#[allow(dead_code)]
pub const WRITE_COALESCE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct WriteRegion {
    pub file_index: usize,
    pub file_offset: u64,
    pub data: Bytes,
}

pub struct FlushRequest {
    pub torrent_hash: String,
    pub piece_index: u32,
    pub regions: Vec<WriteRegion>,
    pub piece_data: Bytes,
    pub expected_hash: Vec<u8>,
}

pub struct WriteCoalescer {
    flush_tx: mpsc::UnboundedSender<FlushRequest>,
}

impl WriteCoalescer {
    pub fn new(flush_tx: mpsc::UnboundedSender<FlushRequest>) -> Self {
        Self { flush_tx }
    }

    pub fn submit_flush(&self, request: FlushRequest) -> bool {
        self.flush_tx.send(request).is_ok()
    }
}

pub fn coalesce_blocks(blocks: &[(u32, Bytes)]) -> Vec<WriteRegion> {
    let mut sorted: Vec<_> = blocks.iter().collect();
    sorted.sort_by_key(|(offset, _)| *offset);

    let mut regions = Vec::new();
    let mut current: Option<(u32, Vec<u8>)> = None;

    for (offset, data) in sorted {
        match &mut current {
            Some((start, buf)) if *start + buf.len() as u32 == *offset => {
                buf.extend_from_slice(data);
            }
            _ => {
                if let Some((start, buf)) = current.take() {
                    regions.push(WriteRegion {
                        file_index: 0,
                        file_offset: start as u64,
                        data: Bytes::from(buf),
                    });
                }
                current = Some((*offset, data.to_vec()));
            }
        }
    }

    if let Some((start, buf)) = current {
        regions.push(WriteRegion {
            file_index: 0,
            file_offset: start as u64,
            data: Bytes::from(buf),
        });
    }

    regions
}
