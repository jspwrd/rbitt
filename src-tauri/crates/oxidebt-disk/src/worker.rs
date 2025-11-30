use crate::io_queue::WriteOp;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

struct WorkerFileHandle {
    file: File,
    last_write_offset: u64,
}

pub struct IoWorker {
    worker_id: usize,
    rx: mpsc::Receiver<Vec<WriteOp>>,
    file_handles: HashMap<(String, usize), WorkerFileHandle>,
    base_paths: HashMap<String, PathBuf>,
    file_paths: HashMap<String, Vec<PathBuf>>,
}

impl IoWorker {
    pub fn new(worker_id: usize, rx: mpsc::Receiver<Vec<WriteOp>>) -> Self {
        Self {
            worker_id,
            rx,
            file_handles: HashMap::new(),
            base_paths: HashMap::new(),
            file_paths: HashMap::new(),
        }
    }

    pub fn register_torrent(&mut self, torrent_hash: String, base_path: PathBuf, files: Vec<PathBuf>) {
        self.base_paths.insert(torrent_hash.clone(), base_path);
        self.file_paths.insert(torrent_hash, files);
    }

    pub async fn run(&mut self) {
        while let Some(ops) = self.rx.recv().await {
            for op in ops {
                if let Err(e) = self.process_write(op).await {
                    tracing::warn!("Worker {} write error: {}", self.worker_id, e);
                }
            }
        }
    }

    async fn process_write(&mut self, op: WriteOp) -> std::io::Result<()> {
        let key = (op.torrent_hash.clone(), op.file_index);

        let handle = match self.file_handles.get_mut(&key) {
            Some(h) => h,
            None => {
                let path = self.get_file_path(&op.torrent_hash, op.file_index)?;
                let file = self.open_file_for_write(&path).await?;
                self.file_handles.insert(
                    key.clone(),
                    WorkerFileHandle {
                        file,
                        last_write_offset: u64::MAX,
                    },
                );
                self.file_handles.get_mut(&key).unwrap()
            }
        };

        if handle.last_write_offset != op.file_offset {
            handle.file.seek(SeekFrom::Start(op.file_offset)).await?;
        }

        handle.file.write_all(&op.data).await?;
        handle.last_write_offset = op.file_offset + op.data.len() as u64;

        Ok(())
    }

    fn get_file_path(&self, torrent_hash: &str, file_index: usize) -> std::io::Result<PathBuf> {
        let base = self
            .base_paths
            .get(torrent_hash)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "torrent not registered"))?;

        let files = self
            .file_paths
            .get(torrent_hash)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "torrent not registered"))?;

        files
            .get(file_index)
            .map(|p| base.join(p))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "file index out of range"))
    }

    async fn open_file_for_write(&self, path: &PathBuf) -> std::io::Result<File> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .await
    }

    pub async fn flush_all(&mut self) {
        for (_, handle) in self.file_handles.iter_mut() {
            let _ = handle.file.sync_data().await;
        }
    }

    pub fn close_all(&mut self) {
        self.file_handles.clear();
    }

    pub fn worker_id(&self) -> usize {
        self.worker_id
    }
}
