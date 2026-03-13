use crate::advanced::Priority;
use crate::task::TaskId;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, VecDeque};

/// Trait for custom task schedulers
pub trait Scheduler: Send + Sync {
    /// Add a task to the scheduler
    fn push(&mut self, task_id: TaskId, priority: Priority);

    /// Get the next task to execute
    fn pop(&mut self) -> Option<TaskId>;

    /// Check if scheduler is empty
    fn is_empty(&self) -> bool;

    /// Get the number of tasks in the scheduler
    fn len(&self) -> usize;
}

/// FIFO scheduler (default - simple queue)
pub struct FifoScheduler {
    queue: VecDeque<TaskId>,
}

impl FifoScheduler {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

impl Default for FifoScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for FifoScheduler {
    fn push(&mut self, task_id: TaskId, _priority: Priority) {
        self.queue.push_back(task_id);
    }

    fn pop(&mut self) -> Option<TaskId> {
        self.queue.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn len(&self) -> usize {
        self.queue.len()
    }
}

/// Priority-based scheduler (priority queue)
pub struct PriorityScheduler {
    heap: BinaryHeap<PriorityTask>,
}

#[derive(Eq, PartialEq)]
struct PriorityTask {
    task_id: TaskId,
    priority: Priority,
    insertion_order: usize,
}

impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Higher priority first
        match self.priority.cmp(&other.priority) {
            CmpOrdering::Equal => {
                // If same priority, use FIFO (lower insertion_order first)
                other.insertion_order.cmp(&self.insertion_order)
            }
            other => other,
        }
    }
}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl PriorityScheduler {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    fn next_insertion_order(&self) -> usize {
        self.heap.len()
    }
}

impl Default for PriorityScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for PriorityScheduler {
    fn push(&mut self, task_id: TaskId, priority: Priority) {
        let insertion_order = self.next_insertion_order();
        self.heap.push(PriorityTask {
            task_id,
            priority,
            insertion_order,
        });
    }

    fn pop(&mut self) -> Option<TaskId> {
        self.heap.pop().map(|pt| pt.task_id)
    }

    fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    fn len(&self) -> usize {
        self.heap.len()
    }
}

/// Round-robin scheduler (distributes tasks evenly)
pub struct RoundRobinScheduler {
    queues: Vec<VecDeque<TaskId>>,
    current: usize,
}

impl RoundRobinScheduler {
    pub fn new(num_workers: usize) -> Self {
        let mut queues = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            queues.push(VecDeque::new());
        }

        Self { queues, current: 0 }
    }
}

impl Scheduler for RoundRobinScheduler {
    fn push(&mut self, task_id: TaskId, _priority: Priority) {
        let queue_idx = self.current % self.queues.len();
        self.queues[queue_idx].push_back(task_id);
        self.current = (self.current + 1) % self.queues.len();
    }

    fn pop(&mut self) -> Option<TaskId> {
        // Try each queue in round-robin fashion
        for _ in 0..self.queues.len() {
            if let Some(task_id) = self.queues[self.current].pop_front() {
                self.current = (self.current + 1) % self.queues.len();
                return Some(task_id);
            }
            self.current = (self.current + 1) % self.queues.len();
        }
        None
    }

    fn is_empty(&self) -> bool {
        self.queues.iter().all(|q| q.is_empty())
    }

    fn len(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_scheduler() {
        let mut sched = FifoScheduler::new();
        assert!(sched.is_empty());

        sched.push(1, Priority::Normal);
        sched.push(2, Priority::High);
        sched.push(3, Priority::Low);

        assert_eq!(sched.len(), 3);
        assert_eq!(sched.pop(), Some(1));
        assert_eq!(sched.pop(), Some(2));
        assert_eq!(sched.pop(), Some(3));
        assert!(sched.is_empty());
    }

    #[test]
    fn test_priority_scheduler() {
        let mut sched = PriorityScheduler::new();

        sched.push(1, Priority::Normal);
        sched.push(2, Priority::High);
        sched.push(3, Priority::Low);
        sched.push(4, Priority::Critical);

        // Should come out in priority order
        assert_eq!(sched.pop(), Some(4)); // Critical
        assert_eq!(sched.pop(), Some(2)); // High
        assert_eq!(sched.pop(), Some(1)); // Normal
        assert_eq!(sched.pop(), Some(3)); // Low
    }

    #[test]
    fn test_priority_scheduler_fifo_fallback() {
        let mut sched = PriorityScheduler::new();

        // Same priority - should use FIFO
        sched.push(1, Priority::Normal);
        sched.push(2, Priority::Normal);
        sched.push(3, Priority::Normal);

        assert_eq!(sched.pop(), Some(1));
        assert_eq!(sched.pop(), Some(2));
        assert_eq!(sched.pop(), Some(3));
    }

    #[test]
    fn test_round_robin_scheduler() {
        let mut sched = RoundRobinScheduler::new(3);

        sched.push(1, Priority::Normal);
        sched.push(2, Priority::Normal);
        sched.push(3, Priority::Normal);
        sched.push(4, Priority::Normal);

        assert_eq!(sched.len(), 4);

        // Should distribute evenly across 3 queues
        let mut results = Vec::new();
        while let Some(task) = sched.pop() {
            results.push(task);
        }

        assert_eq!(results.len(), 4);
        assert!(results.contains(&1));
        assert!(results.contains(&2));
        assert!(results.contains(&3));
        assert!(results.contains(&4));
    }
}
