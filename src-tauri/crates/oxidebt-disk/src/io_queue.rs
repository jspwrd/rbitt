use bytes::Bytes;
use std::cmp::Ordering as CmpOrdering;
use std::time::Duration;
use tokio::sync::mpsc;

pub const IO_BATCH_SIZE: usize = 64;
pub const IO_BATCH_TIMEOUT: Duration = Duration::from_millis(10);
#[allow(dead_code)]
pub const IO_WORKERS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WritePriority {
    High,
    Normal,
    Low,
}

impl WritePriority {
    fn rank(&self) -> u8 {
        match self {
            WritePriority::High => 0,
            WritePriority::Normal => 1,
            WritePriority::Low => 2,
        }
    }
}

pub struct WriteOp {
    pub torrent_hash: String,
    pub file_index: usize,
    pub file_offset: u64,
    pub data: Bytes,
    pub priority: WritePriority,
}

impl WriteOp {
    fn sort_key(&self) -> (u8, usize, u64) {
        (self.priority.rank(), self.file_index, self.file_offset)
    }
}

pub struct IoQueue {
    write_tx: mpsc::Sender<WriteOp>,
    write_rx: Option<mpsc::Receiver<WriteOp>>,
    worker_txs: Vec<mpsc::Sender<Vec<WriteOp>>>,
    batch_size: usize,
    #[allow(dead_code)]
    batch_timeout: Duration,
}

impl IoQueue {
    pub fn new(num_workers: usize) -> (Self, Vec<mpsc::Receiver<Vec<WriteOp>>>) {
        let (write_tx, write_rx) = mpsc::channel(4096);
        let mut worker_txs = Vec::with_capacity(num_workers);
        let mut worker_rxs = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let (tx, rx) = mpsc::channel(64);
            worker_txs.push(tx);
            worker_rxs.push(rx);
        }

        let queue = Self {
            write_tx,
            write_rx: Some(write_rx),
            worker_txs,
            batch_size: IO_BATCH_SIZE,
            batch_timeout: IO_BATCH_TIMEOUT,
        };

        (queue, worker_rxs)
    }

    pub fn submit(&self, op: WriteOp) -> bool {
        self.write_tx.try_send(op).is_ok()
    }

    pub async fn submit_async(&self, op: WriteOp) -> bool {
        self.write_tx.send(op).await.is_ok()
    }

    pub async fn run_dispatcher(&mut self) {
        let mut rx = match self.write_rx.take() {
            Some(rx) => rx,
            None => return,
        };

        let num_workers = self.worker_txs.len();
        let mut batch: Vec<WriteOp> = Vec::with_capacity(self.batch_size);

        loop {
            tokio::select! {
                Some(op) = rx.recv() => {
                    batch.push(op);

                    while batch.len() < self.batch_size {
                        match rx.try_recv() {
                            Ok(op) => batch.push(op),
                            Err(_) => break,
                        }
                    }

                    if !batch.is_empty() {
                        self.dispatch_batch(&mut batch, num_workers).await;
                    }
                }
                _ = tokio::time::sleep(IO_BATCH_TIMEOUT) => {
                    if !batch.is_empty() {
                        self.dispatch_batch(&mut batch, num_workers).await;
                    }
                }
            }
        }
    }

    async fn dispatch_batch(&self, batch: &mut Vec<WriteOp>, num_workers: usize) {
        batch.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

        let mut per_worker: Vec<Vec<WriteOp>> = (0..num_workers).map(|_| Vec::new()).collect();

        for op in batch.drain(..) {
            let worker_idx = op.file_index % num_workers;
            per_worker[worker_idx].push(op);
        }

        for (idx, ops) in per_worker.into_iter().enumerate() {
            if !ops.is_empty() {
                let _ = self.worker_txs[idx].send(ops).await;
            }
        }
    }

    pub fn num_workers(&self) -> usize {
        self.worker_txs.len()
    }
}

#[allow(dead_code)]
pub fn sort_writes_by_offset(ops: &mut [WriteOp]) {
    ops.sort_by(|a, b| match a.file_index.cmp(&b.file_index) {
        CmpOrdering::Equal => a.file_offset.cmp(&b.file_offset),
        other => other,
    });
}
