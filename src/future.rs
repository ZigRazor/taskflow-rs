use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Future returned from executor.run() to track task completion
pub struct TaskflowFuture {
    workers: Vec<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl TaskflowFuture {
    pub(crate) fn new(workers: Vec<JoinHandle<()>>, shutdown: Arc<AtomicBool>) -> Self {
        Self { workers, shutdown }
    }

    /// Wait for the taskflow to complete
    pub fn wait(self) {
        for worker in self.workers {
            let _ = worker.join();
        }
    }

    /// Check if the taskflow is still running
    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::SeqCst)
    }
}
