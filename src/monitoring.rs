use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Performance metrics for taskflow execution
#[derive(Clone)]
pub struct PerformanceMetrics {
    // Counters
    tasks_completed: Arc<AtomicUsize>,
    tasks_stolen: Arc<AtomicUsize>,
    tasks_failed: Arc<AtomicUsize>,

    // Timing (in nanoseconds)
    total_execution_time_ns: Arc<AtomicU64>,
    total_wait_time_ns: Arc<AtomicU64>,

    // Worker stats
    num_workers: usize,
    worker_idle_time_ns: Vec<Arc<AtomicU64>>,
    worker_busy_time_ns: Vec<Arc<AtomicU64>>,

    // Start time
    start_time: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl PerformanceMetrics {
    /// Create new performance metrics
    pub fn new(num_workers: usize) -> Self {
        let mut worker_idle = Vec::new();
        let mut worker_busy = Vec::new();

        for _ in 0..num_workers {
            worker_idle.push(Arc::new(AtomicU64::new(0)));
            worker_busy.push(Arc::new(AtomicU64::new(0)));
        }

        Self {
            tasks_completed: Arc::new(AtomicUsize::new(0)),
            tasks_stolen: Arc::new(AtomicUsize::new(0)),
            tasks_failed: Arc::new(AtomicUsize::new(0)),
            total_execution_time_ns: Arc::new(AtomicU64::new(0)),
            total_wait_time_ns: Arc::new(AtomicU64::new(0)),
            num_workers,
            worker_idle_time_ns: worker_idle,
            worker_busy_time_ns: worker_busy,
            start_time: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Mark execution start
    pub fn start(&self) {
        *self.start_time.lock().unwrap() = Some(Instant::now());
    }

    /// Record task completion
    pub fn record_task_completion(&self, duration: Duration) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// Record task steal
    pub fn record_task_steal(&self) {
        self.tasks_stolen.fetch_add(1, Ordering::Relaxed);
    }

    /// Record task failure
    pub fn record_task_failure(&self) {
        self.tasks_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record worker idle time
    pub fn record_worker_idle(&self, worker_id: usize, duration: Duration) {
        if worker_id < self.num_workers {
            self.worker_idle_time_ns[worker_id]
                .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// Record worker busy time
    pub fn record_worker_busy(&self, worker_id: usize, duration: Duration) {
        if worker_id < self.num_workers {
            self.worker_busy_time_ns[worker_id]
                .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// Get total tasks completed
    pub fn tasks_completed(&self) -> usize {
        self.tasks_completed.load(Ordering::Relaxed)
    }

    /// Get total tasks stolen
    pub fn tasks_stolen(&self) -> usize {
        self.tasks_stolen.load(Ordering::Relaxed)
    }

    /// Get total tasks failed
    pub fn tasks_failed(&self) -> usize {
        self.tasks_failed.load(Ordering::Relaxed)
    }

    /// Get total execution time
    pub fn total_execution_time(&self) -> Duration {
        Duration::from_nanos(self.total_execution_time_ns.load(Ordering::Relaxed))
    }

    /// Get average task duration
    pub fn average_task_duration(&self) -> Duration {
        let completed = self.tasks_completed();
        if completed == 0 {
            return Duration::ZERO;
        }

        let total_ns = self.total_execution_time_ns.load(Ordering::Relaxed);
        Duration::from_nanos(total_ns / completed as u64)
    }

    /// Get tasks per second
    pub fn tasks_per_second(&self) -> f64 {
        let start = self.start_time.lock().unwrap();
        if let Some(start_instant) = *start {
            let elapsed = start_instant.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return self.tasks_completed() as f64 / elapsed;
            }
        }
        0.0
    }

    /// Get worker utilization (percentage busy)
    pub fn worker_utilization(&self, worker_id: usize) -> f64 {
        if worker_id >= self.num_workers {
            return 0.0;
        }

        let idle_ns = self.worker_idle_time_ns[worker_id].load(Ordering::Relaxed);
        let busy_ns = self.worker_busy_time_ns[worker_id].load(Ordering::Relaxed);
        let total_ns = idle_ns + busy_ns;

        if total_ns == 0 {
            return 0.0;
        }

        (busy_ns as f64 / total_ns as f64) * 100.0
    }

    /// Get average worker utilization
    pub fn average_worker_utilization(&self) -> f64 {
        let sum: f64 = (0..self.num_workers)
            .map(|w| self.worker_utilization(w))
            .sum();

        if self.num_workers == 0 {
            0.0
        } else {
            sum / self.num_workers as f64
        }
    }

    /// Get steal rate (percentage of tasks stolen)
    pub fn steal_rate(&self) -> f64 {
        let completed = self.tasks_completed();
        if completed == 0 {
            return 0.0;
        }

        (self.tasks_stolen() as f64 / completed as f64) * 100.0
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.tasks_completed.store(0, Ordering::Relaxed);
        self.tasks_stolen.store(0, Ordering::Relaxed);
        self.tasks_failed.store(0, Ordering::Relaxed);
        self.total_execution_time_ns.store(0, Ordering::Relaxed);
        self.total_wait_time_ns.store(0, Ordering::Relaxed);

        for worker_idle in &self.worker_idle_time_ns {
            worker_idle.store(0, Ordering::Relaxed);
        }

        for worker_busy in &self.worker_busy_time_ns {
            worker_busy.store(0, Ordering::Relaxed);
        }

        *self.start_time.lock().unwrap() = None;
    }

    /// Generate summary report
    pub fn summary(&self) -> String {
        let mut report = String::new();

        report.push_str("=== Performance Metrics ===\n");
        report.push_str(&format!("Tasks Completed: {}\n", self.tasks_completed()));
        report.push_str(&format!(
            "Tasks Stolen: {} ({:.2}%)\n",
            self.tasks_stolen(),
            self.steal_rate()
        ));
        report.push_str(&format!("Tasks Failed: {}\n", self.tasks_failed()));
        report.push_str(&format!(
            "Average Task Duration: {:?}\n",
            self.average_task_duration()
        ));
        report.push_str(&format!("Tasks/Second: {:.2}\n", self.tasks_per_second()));
        report.push_str(&format!(
            "Average Worker Utilization: {:.2}%\n",
            self.average_worker_utilization()
        ));

        report.push_str("\nWorker Utilization:\n");
        for worker_id in 0..self.num_workers {
            report.push_str(&format!(
                "  Worker {}: {:.2}%\n",
                worker_id,
                self.worker_utilization(worker_id)
            ));
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = PerformanceMetrics::new(4);
        assert_eq!(metrics.tasks_completed(), 0);
        assert_eq!(metrics.num_workers, 4);
    }

    #[test]
    fn test_task_completion() {
        let metrics = PerformanceMetrics::new(4);
        metrics.record_task_completion(Duration::from_millis(100));

        assert_eq!(metrics.tasks_completed(), 1);
        assert_eq!(metrics.average_task_duration(), Duration::from_millis(100));
    }

    #[test]
    fn test_worker_utilization() {
        let metrics = PerformanceMetrics::new(2);

        metrics.record_worker_busy(0, Duration::from_secs(8));
        metrics.record_worker_idle(0, Duration::from_secs(2));

        assert_eq!(metrics.worker_utilization(0), 80.0);
    }

    #[test]
    fn test_steal_rate() {
        let metrics = PerformanceMetrics::new(4);

        metrics.record_task_completion(Duration::from_millis(10));
        metrics.record_task_completion(Duration::from_millis(10));
        metrics.record_task_completion(Duration::from_millis(10));
        metrics.record_task_steal();

        assert_eq!(metrics.steal_rate(), 100.0 / 3.0);
    }
}
