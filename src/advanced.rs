use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Task priority levels (higher value = higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Lowest priority (background tasks)
    Low = 0,
    /// Normal priority (default)
    Normal = 5,
    /// High priority (important tasks)
    High = 10,
    /// Critical priority (must run ASAP)
    Critical = 15,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

impl From<u8> for Priority {
    fn from(val: u8) -> Self {
        match val {
            0..=2 => Priority::Low,
            3..=7 => Priority::Normal,
            8..=12 => Priority::High,
            _ => Priority::Critical,
        }
    }
}

impl From<Priority> for u8 {
    fn from(p: Priority) -> u8 {
        p as u8
    }
}

/// Cancellation token for task cancellation
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    cancel_count: Arc<AtomicUsize>,
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel_count: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    /// Cancel this token
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cancel_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Check if this token is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    
    /// Reset the cancellation (allow reuse)
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
    }
    
    /// Get the number of times this has been cancelled
    pub fn cancel_count(&self) -> usize {
        self.cancel_count.load(Ordering::Relaxed)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Task metadata for advanced features
#[derive(Clone)]
pub struct TaskMetadata {
    /// Task priority
    pub priority: Priority,
    /// Optional cancellation token
    pub cancellation_token: Option<CancellationToken>,
    /// NUMA node hint (-1 for any)
    pub numa_node: i32,
}

impl Default for TaskMetadata {
    fn default() -> Self {
        Self {
            priority: Priority::Normal,
            cancellation_token: None,
            numa_node: -1,
        }
    }
}

impl TaskMetadata {
    /// Create with priority
    pub fn with_priority(priority: Priority) -> Self {
        Self {
            priority,
            ..Default::default()
        }
    }
    
    /// Create with cancellation token
    pub fn with_cancellation(token: CancellationToken) -> Self {
        Self {
            cancellation_token: Some(token),
            ..Default::default()
        }
    }
    
    /// Create with NUMA node hint
    pub fn with_numa_node(node: i32) -> Self {
        Self {
            numa_node: node,
            ..Default::default()
        }
    }
    
    /// Check if task should be cancelled
    pub fn should_cancel(&self) -> bool {
        self.cancellation_token
            .as_ref()
            .map_or(false, |t| t.is_cancelled())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }
    
    #[test]
    fn test_priority_conversion() {
        assert_eq!(Priority::from(0), Priority::Low);
        assert_eq!(Priority::from(5), Priority::Normal);
        assert_eq!(Priority::from(10), Priority::High);
        assert_eq!(Priority::from(20), Priority::Critical);
    }
    
    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        
        token.cancel();
        assert!(token.is_cancelled());
        assert_eq!(token.cancel_count(), 1);
        
        token.reset();
        assert!(!token.is_cancelled());
    }
    
    #[test]
    fn test_task_metadata() {
        let meta = TaskMetadata::with_priority(Priority::High);
        assert_eq!(meta.priority, Priority::High);
        assert!(!meta.should_cancel());
        
        let token = CancellationToken::new();
        let meta = TaskMetadata::with_cancellation(token.clone());
        assert!(!meta.should_cancel());
        
        token.cancel();
        assert!(meta.should_cancel());
    }
}
