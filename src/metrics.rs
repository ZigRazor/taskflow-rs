use crate::task::TaskId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Comprehensive metrics collection system
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    // Execution metrics
    total_tasks_executed: AtomicUsize,
    total_tasks_failed: AtomicUsize,
    total_execution_time_ns: AtomicU64,

    // Task timing
    task_timings: Mutex<HashMap<TaskId, Duration>>,
    task_start_times: Mutex<HashMap<TaskId, Instant>>,

    // Worker metrics
    num_workers: usize,
    worker_busy_time_ns: Vec<AtomicU64>,
    worker_idle_time_ns: Vec<AtomicU64>,
    worker_tasks_executed: Vec<AtomicUsize>,

    // Throughput metrics
    tasks_per_second_samples: Mutex<Vec<f64>>,

    // Resource metrics
    peak_memory_bytes: AtomicU64,
    current_memory_bytes: AtomicU64,

    // Start time
    start_time: Mutex<Option<Instant>>,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new(num_workers: usize) -> Self {
        let mut worker_busy_time_ns = Vec::new();
        let mut worker_idle_time_ns = Vec::new();
        let mut worker_tasks_executed = Vec::new();

        for _ in 0..num_workers {
            worker_busy_time_ns.push(AtomicU64::new(0));
            worker_idle_time_ns.push(AtomicU64::new(0));
            worker_tasks_executed.push(AtomicUsize::new(0));
        }

        Self {
            inner: Arc::new(MetricsInner {
                total_tasks_executed: AtomicUsize::new(0),
                total_tasks_failed: AtomicUsize::new(0),
                total_execution_time_ns: AtomicU64::new(0),
                task_timings: Mutex::new(HashMap::new()),
                task_start_times: Mutex::new(HashMap::new()),
                num_workers,
                worker_busy_time_ns,
                worker_idle_time_ns,
                worker_tasks_executed,
                tasks_per_second_samples: Mutex::new(Vec::new()),
                peak_memory_bytes: AtomicU64::new(0),
                current_memory_bytes: AtomicU64::new(0),
                start_time: Mutex::new(None),
            }),
        }
    }

    /// Start metrics collection
    pub fn start(&self) {
        *self.inner.start_time.lock().unwrap() = Some(Instant::now());
    }

    /// Record task start
    pub fn record_task_start(&self, task_id: TaskId) {
        self.inner
            .task_start_times
            .lock()
            .unwrap()
            .insert(task_id, Instant::now());
    }

    /// Record task completion
    pub fn record_task_completion(&self, task_id: TaskId, worker_id: usize) {
        let duration = if let Some(start_time) =
            self.inner.task_start_times.lock().unwrap().remove(&task_id)
        {
            start_time.elapsed()
        } else {
            Duration::from_secs(0)
        };

        // Update task timing
        self.inner
            .task_timings
            .lock()
            .unwrap()
            .insert(task_id, duration);

        // Update counters
        self.inner
            .total_tasks_executed
            .fetch_add(1, Ordering::Relaxed);
        self.inner
            .total_execution_time_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);

        // Update worker metrics
        if worker_id < self.inner.num_workers {
            self.inner.worker_busy_time_ns[worker_id]
                .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
            self.inner.worker_tasks_executed[worker_id].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record task failure
    pub fn record_task_failure(&self, task_id: TaskId) {
        self.inner.task_start_times.lock().unwrap().remove(&task_id);
        self.inner
            .total_tasks_failed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record worker idle time
    pub fn record_worker_idle(&self, worker_id: usize, duration: Duration) {
        if worker_id < self.inner.num_workers {
            self.inner.worker_idle_time_ns[worker_id]
                .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// Record memory usage
    pub fn record_memory_usage(&self, bytes: u64) {
        self.inner
            .current_memory_bytes
            .store(bytes, Ordering::Relaxed);

        // Update peak
        let mut peak = self.inner.peak_memory_bytes.load(Ordering::Relaxed);
        while bytes > peak {
            match self.inner.peak_memory_bytes.compare_exchange(
                peak,
                bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }

    /// Get total tasks executed
    pub fn total_tasks_executed(&self) -> usize {
        self.inner.total_tasks_executed.load(Ordering::Relaxed)
    }

    /// Get total tasks failed
    pub fn total_tasks_failed(&self) -> usize {
        self.inner.total_tasks_failed.load(Ordering::Relaxed)
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.total_tasks_executed() + self.total_tasks_failed();
        if total == 0 {
            return 1.0;
        }
        self.total_tasks_executed() as f64 / total as f64
    }

    /// Get average task duration
    pub fn average_task_duration(&self) -> Duration {
        let total = self.inner.total_tasks_executed.load(Ordering::Relaxed);
        if total == 0 {
            return Duration::from_secs(0);
        }

        let total_ns = self.inner.total_execution_time_ns.load(Ordering::Relaxed);
        Duration::from_nanos(total_ns / total as u64)
    }

    /// Get tasks per second
    pub fn tasks_per_second(&self) -> f64 {
        if let Some(start) = *self.inner.start_time.lock().unwrap() {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return self.total_tasks_executed() as f64 / elapsed;
            }
        }
        0.0
    }

    /// Get worker utilization (0.0 to 1.0)
    pub fn worker_utilization(&self, worker_id: usize) -> f64 {
        if worker_id >= self.inner.num_workers {
            return 0.0;
        }

        let busy_ns = self.inner.worker_busy_time_ns[worker_id].load(Ordering::Relaxed);
        let idle_ns = self.inner.worker_idle_time_ns[worker_id].load(Ordering::Relaxed);
        let total_ns = busy_ns + idle_ns;

        if total_ns == 0 {
            return 0.0;
        }

        busy_ns as f64 / total_ns as f64
    }

    /// Get average worker utilization
    pub fn average_worker_utilization(&self) -> f64 {
        let mut sum = 0.0;
        for i in 0..self.inner.num_workers {
            sum += self.worker_utilization(i);
        }
        sum / self.inner.num_workers as f64
    }

    /// Get worker task count
    pub fn worker_task_count(&self, worker_id: usize) -> usize {
        if worker_id >= self.inner.num_workers {
            return 0;
        }
        self.inner.worker_tasks_executed[worker_id].load(Ordering::Relaxed)
    }

    /// Get memory usage
    pub fn current_memory_bytes(&self) -> u64 {
        self.inner.current_memory_bytes.load(Ordering::Relaxed)
    }

    /// Get peak memory usage
    pub fn peak_memory_bytes(&self) -> u64 {
        self.inner.peak_memory_bytes.load(Ordering::Relaxed)
    }

    /// Get task timing histogram
    pub fn task_timing_histogram(&self) -> Vec<(Duration, usize)> {
        let timings = self.inner.task_timings.lock().unwrap();
        let mut buckets: HashMap<u64, usize> = HashMap::new();

        for duration in timings.values() {
            let bucket = (duration.as_micros() / 100) * 100; // 100μs buckets
            *buckets.entry(bucket as u64).or_insert(0) += 1;
        }

        let mut result: Vec<_> = buckets
            .into_iter()
            .map(|(bucket, count)| (Duration::from_micros(bucket), count))
            .collect();

        result.sort_by_key(|(d, _)| *d);
        result
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            total_tasks_executed: self.total_tasks_executed(),
            total_tasks_failed: self.total_tasks_failed(),
            success_rate: self.success_rate(),
            average_task_duration: self.average_task_duration(),
            tasks_per_second: self.tasks_per_second(),
            average_worker_utilization: self.average_worker_utilization(),
            current_memory_mb: self.current_memory_bytes() as f64 / 1024.0 / 1024.0,
            peak_memory_mb: self.peak_memory_bytes() as f64 / 1024.0 / 1024.0,
            num_workers: self.inner.num_workers,
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.inner.total_tasks_executed.store(0, Ordering::Relaxed);
        self.inner.total_tasks_failed.store(0, Ordering::Relaxed);
        self.inner
            .total_execution_time_ns
            .store(0, Ordering::Relaxed);

        self.inner.task_timings.lock().unwrap().clear();
        self.inner.task_start_times.lock().unwrap().clear();

        for i in 0..self.inner.num_workers {
            self.inner.worker_busy_time_ns[i].store(0, Ordering::Relaxed);
            self.inner.worker_idle_time_ns[i].store(0, Ordering::Relaxed);
            self.inner.worker_tasks_executed[i].store(0, Ordering::Relaxed);
        }

        self.inner.tasks_per_second_samples.lock().unwrap().clear();
        self.inner.peak_memory_bytes.store(0, Ordering::Relaxed);
        self.inner.current_memory_bytes.store(0, Ordering::Relaxed);

        *self.inner.start_time.lock().unwrap() = Some(Instant::now());
    }
}

/// Summary of metrics
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub total_tasks_executed: usize,
    pub total_tasks_failed: usize,
    pub success_rate: f64,
    pub average_task_duration: Duration,
    pub tasks_per_second: f64,
    pub average_worker_utilization: f64,
    pub current_memory_mb: f64,
    pub peak_memory_mb: f64,
    pub num_workers: usize,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Metrics Summary ===")?;
        writeln!(f, "Tasks Executed: {}", self.total_tasks_executed)?;
        writeln!(f, "Tasks Failed: {}", self.total_tasks_failed)?;
        writeln!(f, "Success Rate: {:.2}%", self.success_rate * 100.0)?;
        writeln!(f, "Average Task Duration: {:?}", self.average_task_duration)?;
        writeln!(f, "Tasks/Second: {:.2}", self.tasks_per_second)?;
        writeln!(
            f,
            "Worker Utilization: {:.2}%",
            self.average_worker_utilization * 100.0
        )?;
        writeln!(f, "Current Memory: {:.2} MB", self.current_memory_mb)?;
        writeln!(f, "Peak Memory: {:.2} MB", self.peak_memory_mb)?;
        writeln!(f, "Workers: {}", self.num_workers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_basic() {
        let metrics = Metrics::new(4);
        metrics.start();

        metrics.record_task_start(1);
        std::thread::sleep(Duration::from_millis(10));
        metrics.record_task_completion(1, 0);

        assert_eq!(metrics.total_tasks_executed(), 1);
        assert_eq!(metrics.total_tasks_failed(), 0);
        assert_eq!(metrics.success_rate(), 1.0);
    }

    #[test]
    fn test_worker_metrics() {
        let metrics = Metrics::new(2);

        metrics.record_task_start(1);
        std::thread::sleep(Duration::from_millis(10));
        metrics.record_task_completion(1, 0);

        metrics.record_task_start(2);
        std::thread::sleep(Duration::from_millis(10));
        metrics.record_task_completion(2, 1);

        assert_eq!(metrics.worker_task_count(0), 1);
        assert_eq!(metrics.worker_task_count(1), 1);
    }
}
