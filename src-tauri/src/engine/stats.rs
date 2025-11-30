use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Window size for rate calculation (10 seconds provides good smoothing)
const RATE_WINDOW: Duration = Duration::from_secs(10);

/// How often to sample (100ms granularity)
const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

/// Smoothing factor for exponential moving average (0.1 = smooth, 0.3 = responsive)
const EMA_ALPHA: f64 = 0.15;

/// A sample of bytes transferred in a time bucket
#[derive(Debug, Clone, Copy)]
struct RateSample {
    timestamp: Instant,
    bytes: u64,
}

/// Calculates transfer rates using a sliding window with exponential smoothing.
///
/// This implementation:
/// - Uses fixed-size time buckets (100ms) to limit memory usage
/// - Maintains a sliding window of samples (default 10 seconds)
/// - Applies exponential moving average for smooth rate display
/// - Handles bursty traffic gracefully
pub struct RateCalculator {
    /// Circular buffer of recent samples
    samples: VecDeque<RateSample>,
    /// Last recorded total bytes
    last_total: u64,
    /// Timestamp of last sample
    last_sample_time: Option<Instant>,
    /// Exponentially smoothed rate (bytes/sec)
    smoothed_rate: f64,
    /// Raw instantaneous rate for comparison
    instant_rate: f64,
}

impl RateCalculator {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(128),
            last_total: 0,
            last_sample_time: None,
            smoothed_rate: 0.0,
            instant_rate: 0.0,
        }
    }

    /// Records new byte count and updates rate calculations.
    pub fn update(&mut self, current_total: u64, now: Instant) {
        let delta_bytes = current_total.saturating_sub(self.last_total);

        // Always update the total
        self.last_total = current_total;

        // Check if we should create a new sample bucket
        let should_sample = match self.last_sample_time {
            None => true,
            Some(last) => now.duration_since(last) >= SAMPLE_INTERVAL,
        };

        if should_sample && delta_bytes > 0 {
            // Add new sample
            self.samples.push_back(RateSample {
                timestamp: now,
                bytes: delta_bytes,
            });
            self.last_sample_time = Some(now);
        } else if should_sample {
            // Add zero sample to maintain timing accuracy during idle periods
            self.samples.push_back(RateSample {
                timestamp: now,
                bytes: 0,
            });
            self.last_sample_time = Some(now);
        } else if delta_bytes > 0 {
            // Accumulate bytes into the current bucket
            if let Some(last_sample) = self.samples.back_mut() {
                last_sample.bytes += delta_bytes;
            }
        }

        // Prune old samples outside the window
        let cutoff = now - RATE_WINDOW;
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Calculate instantaneous rate from recent samples
        self.calculate_rates(now);
    }

    fn calculate_rates(&mut self, now: Instant) {
        if self.samples.is_empty() {
            self.instant_rate = 0.0;
            // Decay smoothed rate towards zero when idle
            self.smoothed_rate *= 0.95;
            if self.smoothed_rate < 100.0 {
                self.smoothed_rate = 0.0;
            }
            return;
        }

        // Calculate total bytes and time span in the window
        let total_bytes: u64 = self.samples.iter().map(|s| s.bytes).sum();

        // Check if there's been any recent activity (last 2 seconds)
        let recent_activity = self.samples.iter().rev().take(20).any(|s| s.bytes > 0);

        if !recent_activity && total_bytes == 0 {
            // No recent activity at all - fast decay
            self.instant_rate = 0.0;
            self.smoothed_rate *= 0.8;
            if self.smoothed_rate < 100.0 {
                self.smoothed_rate = 0.0;
            }
            return;
        }

        // Get the actual time span covered by samples
        let first_time = self.samples.front().map(|s| s.timestamp).unwrap_or(now);

        // Use window duration or actual span, whichever is more accurate
        let span = now.duration_since(first_time);
        let elapsed_secs = span.as_secs_f64().max(0.1); // Minimum 100ms to avoid division issues

        // Calculate instantaneous rate
        self.instant_rate = total_bytes as f64 / elapsed_secs;

        // If no recent activity but we have old data, decay faster
        if !recent_activity {
            // Recent samples are all zero, use faster decay
            self.smoothed_rate = 0.3 * self.instant_rate + 0.7 * self.smoothed_rate;
        } else if self.smoothed_rate == 0.0 {
            // Initialize with first measurement
            self.smoothed_rate = self.instant_rate;
        } else {
            // Normal EMA: new_value = alpha * current + (1 - alpha) * previous
            self.smoothed_rate =
                EMA_ALPHA * self.instant_rate + (1.0 - EMA_ALPHA) * self.smoothed_rate;
        }
    }

    /// Returns the smoothed rate in bytes per second.
    pub fn rate(&self) -> f64 {
        self.smoothed_rate
    }

    /// Returns the raw instantaneous rate (for debugging).
    #[allow(dead_code)]
    pub fn instant_rate(&self) -> f64 {
        self.instant_rate
    }

    /// Resets the calculator state.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.samples.clear();
        self.last_total = 0;
        self.last_sample_time = None;
        self.smoothed_rate = 0.0;
        self.instant_rate = 0.0;
    }

    /// Sets the baseline total without counting as transferred bytes.
    /// Use this when restoring state from disk or after verification.
    pub fn set_baseline(&mut self, total: u64) {
        self.last_total = total;
        // Clear any samples since they're now relative to the old baseline
        self.samples.clear();
        self.last_sample_time = None;
        self.smoothed_rate = 0.0;
        self.instant_rate = 0.0;
    }
}

impl Default for RateCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-torrent statistics with smoothed rate tracking.
pub struct TorrentStats {
    pub downloaded: u64,
    pub uploaded: u64,
    pub download_rate: f64,
    pub upload_rate: f64,
    download_rate_calc: RateCalculator,
    upload_rate_calc: RateCalculator,
}

impl TorrentStats {
    pub fn new() -> Self {
        Self {
            downloaded: 0,
            uploaded: 0,
            download_rate: 0.0,
            upload_rate: 0.0,
            download_rate_calc: RateCalculator::new(),
            upload_rate_calc: RateCalculator::new(),
        }
    }

    /// Updates rate calculations based on current byte totals.
    pub fn update_rates(&mut self) {
        let now = Instant::now();
        self.download_rate_calc.update(self.downloaded, now);
        self.upload_rate_calc.update(self.uploaded, now);
        self.download_rate = self.download_rate_calc.rate();
        self.upload_rate = self.upload_rate_calc.rate();

        // Debug log if we have significant upload activity
        if self.uploaded > 0 && self.upload_rate > 0.0 {
            tracing::trace!(
                "Stats update: uploaded={} bytes, upload_rate={:.0} B/s",
                self.uploaded,
                self.upload_rate
            );
        }
    }

    /// Records downloaded bytes and immediately updates rate.
    #[allow(dead_code)]
    pub fn record_download(&mut self, bytes: u64) {
        self.downloaded += bytes;
        let now = Instant::now();
        self.download_rate_calc.update(self.downloaded, now);
        self.download_rate = self.download_rate_calc.rate();
    }

    /// Records uploaded bytes and immediately updates rate.
    #[allow(dead_code)]
    pub fn record_upload(&mut self, bytes: u64) {
        self.uploaded += bytes;
        let now = Instant::now();
        self.upload_rate_calc.update(self.uploaded, now);
        self.upload_rate = self.upload_rate_calc.rate();
    }

    /// Sets the downloaded byte count without affecting the rate calculation.
    /// Use this after verification to set the baseline for already-present data.
    pub fn set_downloaded_baseline(&mut self, bytes: u64) {
        self.downloaded = self.downloaded.max(bytes);
        self.download_rate_calc.set_baseline(self.downloaded);
        self.download_rate = 0.0;
    }
}

impl Default for TorrentStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_calculator_basic() {
        let mut calc = RateCalculator::new();
        let start = Instant::now();

        // Simulate downloading 1MB over ~1 second
        for i in 0..10 {
            calc.update((i + 1) * 100_000, start + Duration::from_millis(i * 100));
        }

        let rate = calc.rate();
        // Should be approximately 1MB/s (within reasonable tolerance)
        assert!(rate > 500_000.0 && rate < 2_000_000.0, "Rate was {}", rate);
    }

    #[test]
    fn test_rate_calculator_smoothing() {
        let mut calc = RateCalculator::new();
        let start = Instant::now();

        // Burst of data
        calc.update(1_000_000, start);
        let rate1 = calc.rate();

        // Wait and add more
        calc.update(1_000_000, start + Duration::from_millis(500));
        let rate2 = calc.rate();

        // Rate should decrease smoothly, not jump
        assert!(
            rate2 <= rate1,
            "Rate should decrease: {} vs {}",
            rate2,
            rate1
        );
    }

    #[test]
    fn test_rate_calculator_idle_decay() {
        let mut calc = RateCalculator::new();
        let start = Instant::now();

        // Simulate some data transfer over time
        for i in 0..10 {
            calc.update((i + 1) * 100_000, start + Duration::from_millis(i * 100));
        }
        let rate1 = calc.rate();
        assert!(rate1 > 0.0, "Should have non-zero rate");

        // Idle for longer than the window - samples will be pruned
        calc.update(1_000_000, start + Duration::from_secs(15));
        let rate2 = calc.rate();

        // Rate should have decayed significantly since samples are outside window
        assert!(
            rate2 < rate1 * 0.9,
            "Rate should decay: {} vs {}",
            rate2,
            rate1
        );
    }
}
