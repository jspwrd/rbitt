
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

pub struct RateLimiter {
    tokens: Mutex<TokenBucket>,
    semaphore: Semaphore,
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    tokens_per_sec: f64,
    last_update: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Arc<Self> {
        let max_tokens = (bytes_per_sec * 2) as f64;
        Arc::new(Self {
            tokens: Mutex::new(TokenBucket {
                tokens: max_tokens,
                max_tokens,
                tokens_per_sec: bytes_per_sec as f64,
                last_update: Instant::now(),
            }),
            semaphore: Semaphore::new(1),
        })
    }

    pub fn unlimited() -> Arc<Self> {
        Arc::new(Self {
            tokens: Mutex::new(TokenBucket {
                tokens: f64::MAX,
                max_tokens: f64::MAX,
                tokens_per_sec: f64::MAX,
                last_update: Instant::now(),
            }),
            semaphore: Semaphore::new(1),
        })
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        let mut bucket = self.tokens.lock();
        bucket.tokens_per_sec = bytes_per_sec as f64;
        bucket.max_tokens = (bytes_per_sec * 2) as f64;
        bucket.tokens = bucket.tokens.min(bucket.max_tokens);
    }

    pub async fn acquire(&self, bytes: usize) -> Duration {
        let _permit = self.semaphore.acquire().await.unwrap();

        let mut bucket = self.tokens.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_update).as_secs_f64();
        bucket.last_update = now;

        bucket.tokens = (bucket.tokens + elapsed * bucket.tokens_per_sec).min(bucket.max_tokens);

        let bytes_f = bytes as f64;
        if bucket.tokens >= bytes_f {
            bucket.tokens -= bytes_f;
            Duration::ZERO
        } else {
            let needed = bytes_f - bucket.tokens;
            let wait_secs = needed / bucket.tokens_per_sec;
            bucket.tokens = 0.0;
            Duration::from_secs_f64(wait_secs)
        }
    }

    pub fn available(&self) -> usize {
        let bucket = self.tokens.lock();
        bucket.tokens as usize
    }
}

pub struct BandwidthLimiter {
    download: Arc<RateLimiter>,
    upload: Arc<RateLimiter>,
}

impl BandwidthLimiter {
    pub fn new(download_limit: u64, upload_limit: u64) -> Self {
        Self {
            download: if download_limit == 0 {
                RateLimiter::unlimited()
            } else {
                RateLimiter::new(download_limit)
            },
            upload: if upload_limit == 0 {
                RateLimiter::unlimited()
            } else {
                RateLimiter::new(upload_limit)
            },
        }
    }

    pub fn unlimited() -> Self {
        Self {
            download: RateLimiter::unlimited(),
            upload: RateLimiter::unlimited(),
        }
    }

    pub fn set_download_limit(&mut self, bytes_per_sec: u64) {
        if bytes_per_sec == 0 {
            self.download = RateLimiter::unlimited();
        } else {
            self.download = RateLimiter::new(bytes_per_sec);
        }
    }

    pub fn set_upload_limit(&mut self, bytes_per_sec: u64) {
        if bytes_per_sec == 0 {
            self.upload = RateLimiter::unlimited();
        } else {
            self.upload = RateLimiter::new(bytes_per_sec);
        }
    }

    pub async fn acquire_download(&self, bytes: usize) {
        let wait = self.download.acquire(bytes).await;
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    pub async fn acquire_upload(&self, bytes: usize) {
        let wait = self.upload.acquire(bytes).await;
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    pub fn download_limiter(&self) -> Arc<RateLimiter> {
        self.download.clone()
    }

    pub fn upload_limiter(&self) -> Arc<RateLimiter> {
        self.upload.clone()
    }
}

impl Default for BandwidthLimiter {
    fn default() -> Self {
        Self::unlimited()
    }
}
