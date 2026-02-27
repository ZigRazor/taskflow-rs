use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::task::TaskId;

/// Task execution statistics
#[derive(Debug, Clone)]
pub struct TaskStats {
    /// Task ID
    pub task_id: TaskId,
    /// Task name (if provided)
    pub name: Option<String>,
    /// Execution start time
    pub start_time: Instant,
    /// Execution duration
    pub duration: Duration,
    /// Worker thread that executed this task
    pub worker_id: usize,
    /// Number of dependencies
    pub num_dependencies: usize,
}

/// Execution profile for a taskflow run
#[derive(Debug, Clone)]
pub struct ExecutionProfile {
    /// Start time of the execution
    pub start_time: Instant,
    /// Total execution time
    pub total_duration: Duration,
    /// Statistics for each task
    pub task_stats: Vec<TaskStats>,
    /// Number of workers used
    pub num_workers: usize,
}

impl ExecutionProfile {
    /// Get the critical path (longest dependency chain)
    pub fn critical_path_duration(&self) -> Duration {
        self.task_stats
            .iter()
            .map(|s| s.duration)
            .max()
            .unwrap_or(Duration::ZERO)
    }
    
    /// Get average task duration
    pub fn average_task_duration(&self) -> Duration {
        if self.task_stats.is_empty() {
            return Duration::ZERO;
        }
        
        let total: Duration = self.task_stats.iter().map(|s| s.duration).sum();
        total / self.task_stats.len() as u32
    }
    
    /// Get the longest running task
    pub fn longest_task(&self) -> Option<&TaskStats> {
        self.task_stats.iter().max_by_key(|s| s.duration)
    }
    
    /// Get the shortest running task
    pub fn shortest_task(&self) -> Option<&TaskStats> {
        self.task_stats.iter().min_by_key(|s| s.duration)
    }
    
    /// Calculate parallelism efficiency
    pub fn parallelism_efficiency(&self) -> f64 {
        if self.total_duration.as_secs_f64() == 0.0 {
            return 0.0;
        }
        
        let total_work: Duration = self.task_stats.iter().map(|s| s.duration).sum();
        let total_work_secs = total_work.as_secs_f64();
        let total_time_secs = self.total_duration.as_secs_f64();
        
        (total_work_secs / (total_time_secs * self.num_workers as f64)) * 100.0
    }
    
    /// Get task execution timeline by worker
    pub fn worker_timeline(&self) -> HashMap<usize, Vec<&TaskStats>> {
        let mut timeline: HashMap<usize, Vec<&TaskStats>> = HashMap::new();
        
        for stats in &self.task_stats {
            timeline.entry(stats.worker_id)
                .or_insert_with(Vec::new)
                .push(stats);
        }
        
        // Sort by start time
        for tasks in timeline.values_mut() {
            tasks.sort_by_key(|s| s.start_time);
        }
        
        timeline
    }
    
    /// Generate a summary report
    pub fn summary(&self) -> String {
        let mut report = String::new();
        
        report.push_str("=== Execution Profile Summary ===\n");
        report.push_str(&format!("Total Duration: {:?}\n", self.total_duration));
        report.push_str(&format!("Tasks Executed: {}\n", self.task_stats.len()));
        report.push_str(&format!("Workers Used: {}\n", self.num_workers));
        report.push_str(&format!("Average Task Duration: {:?}\n", self.average_task_duration()));
        report.push_str(&format!("Parallelism Efficiency: {:.2}%\n", self.parallelism_efficiency()));
        
        if let Some(longest) = self.longest_task() {
            report.push_str(&format!(
                "Longest Task: {} ({:?})\n",
                longest.name.as_ref().unwrap_or(&format!("task_{}", longest.task_id)),
                longest.duration
            ));
        }
        
        if let Some(shortest) = self.shortest_task() {
            report.push_str(&format!(
                "Shortest Task: {} ({:?})\n",
                shortest.name.as_ref().unwrap_or(&format!("task_{}", shortest.task_id)),
                shortest.duration
            ));
        }
        
        report
    }
}

/// Profiler for collecting execution statistics
pub struct Profiler {
    enabled: Arc<Mutex<bool>>,
    task_stats: Arc<Mutex<Vec<TaskStats>>>,
    start_time: Arc<Mutex<Option<Instant>>>,
    num_workers: usize,
}

impl Profiler {
    /// Create a new profiler
    pub fn new(num_workers: usize) -> Self {
        Self {
            enabled: Arc::new(Mutex::new(false)),
            task_stats: Arc::new(Mutex::new(Vec::new())),
            start_time: Arc::new(Mutex::new(None)),
            num_workers,
        }
    }
    
    /// Enable profiling
    pub fn enable(&self) {
        *self.enabled.lock().unwrap() = true;
    }
    
    /// Disable profiling
    pub fn disable(&self) {
        *self.enabled.lock().unwrap() = false;
    }
    
    /// Check if profiling is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }
    
    /// Start profiling a run
    pub fn start_run(&self) {
        if self.is_enabled() {
            *self.start_time.lock().unwrap() = Some(Instant::now());
            self.task_stats.lock().unwrap().clear();
        }
    }
    
    /// Record task execution
    pub fn record_task(
        &self,
        task_id: TaskId,
        name: Option<String>,
        start_time: Instant,
        duration: Duration,
        worker_id: usize,
        num_dependencies: usize,
    ) {
        if self.is_enabled() {
            let stats = TaskStats {
                task_id,
                name,
                start_time,
                duration,
                worker_id,
                num_dependencies,
            };
            
            self.task_stats.lock().unwrap().push(stats);
        }
    }
    
    /// Get the execution profile
    pub fn get_profile(&self) -> Option<ExecutionProfile> {
        if !self.is_enabled() {
            return None;
        }
        
        let start = self.start_time.lock().unwrap().clone()?;
        let stats = self.task_stats.lock().unwrap().clone();
        
        if stats.is_empty() {
            return None;
        }
        
        let end_time = stats.iter()
            .map(|s| s.start_time + s.duration)
            .max()
            .unwrap_or(Instant::now());
        
        Some(ExecutionProfile {
            start_time: start,
            total_duration: end_time - start,
            task_stats: stats,
            num_workers: self.num_workers,
        })
    }
    
    /// Reset profiling data
    pub fn reset(&self) {
        self.task_stats.lock().unwrap().clear();
        *self.start_time.lock().unwrap() = None;
    }
}

impl Clone for Profiler {
    fn clone(&self) -> Self {
        Self {
            enabled: Arc::clone(&self.enabled),
            task_stats: Arc::clone(&self.task_stats),
            start_time: Arc::clone(&self.start_time),
            num_workers: self.num_workers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profiler_enable_disable() {
        let profiler = Profiler::new(4);
        assert!(!profiler.is_enabled());
        
        profiler.enable();
        assert!(profiler.is_enabled());
        
        profiler.disable();
        assert!(!profiler.is_enabled());
    }
    
    #[test]
    fn test_execution_profile() {
        let profiler = Profiler::new(4);
        profiler.enable();
        profiler.start_run();
        
        let start = Instant::now();
        profiler.record_task(1, Some("task1".to_string()), start, Duration::from_millis(100), 0, 0);
        profiler.record_task(2, Some("task2".to_string()), start, Duration::from_millis(50), 1, 1);
        
        let profile = profiler.get_profile().unwrap();
        assert_eq!(profile.task_stats.len(), 2);
        assert_eq!(profile.num_workers, 4);
    }
    
    #[test]
    fn test_profile_statistics() {
        let start = Instant::now();
        let profile = ExecutionProfile {
            start_time: start,
            total_duration: Duration::from_secs(1),
            task_stats: vec![
                TaskStats {
                    task_id: 1,
                    name: Some("task1".to_string()),
                    start_time: start,
                    duration: Duration::from_millis(100),
                    worker_id: 0,
                    num_dependencies: 0,
                },
                TaskStats {
                    task_id: 2,
                    name: Some("task2".to_string()),
                    start_time: start,
                    duration: Duration::from_millis(200),
                    worker_id: 1,
                    num_dependencies: 1,
                },
            ],
            num_workers: 4,
        };
        
        assert_eq!(profile.longest_task().unwrap().task_id, 2);
        assert_eq!(profile.shortest_task().unwrap().task_id, 1);
        assert_eq!(profile.average_task_duration(), Duration::from_millis(150));
    }
}
