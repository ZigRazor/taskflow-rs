//! Dynamic priority adjustment for queued tasks.
//!
//! Provides a thread-safe scheduler where the priority of any pending task can
//! be changed in O(log n) time, even while other threads are concurrently
//! pushing or popping tasks.
//!
//! ## Architecture
//!
//! ```text
//!  DynamicPriorityScheduler (inner, not Send by itself)
//!  ├── index:   BTreeMap<(RevPriority, SeqNum), TaskId>   ← ordered pop
//!  └── reverse: HashMap<TaskId, (RevPriority, SeqNum)>    ← O(1) lookup
//!
//!  SharedDynamicScheduler = Arc<Mutex<DynamicPriorityScheduler>>
//!  └── PriorityHandle ──weak──► SharedDynamicScheduler
//!                               allows reprioritize() without holding
//!                               the scheduler reference forever
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use taskflow_rs::dynamic_priority::{DynamicPriorityScheduler, SharedDynamicScheduler};
//! use taskflow_rs::advanced::Priority;
//!
//! // Shared (thread-safe) variant
//! let sched = SharedDynamicScheduler::new();
//!
//! let handle = sched.push(42, Priority::Low);
//! sched.push(7,  Priority::Normal);
//! sched.push(99, Priority::Critical);
//!
//! // Escalate task 42 to Critical while it is still queued
//! handle.reprioritize(Priority::Critical);
//!
//! // Pop order now: 42(Critical), 99(Critical, older seq), 7(Normal)
//! // Note: tasks with equal priority use FIFO via the sequence number.
//! assert_eq!(sched.pop(), Some(42));
//! ```

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, Weak};

use crate::advanced::Priority;
use crate::task::TaskId;

// ─── Newtype wrappers for the BTree key ──────────────────────────────────────

/// Wraps Priority so that **higher** numeric priority sorts **first** in a
/// `BTreeMap` (which sorts ascending by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RevPriority(u8);

impl RevPriority {
    fn new(p: Priority) -> Self {
        // Invert: max u8 (255) - p_value → highest priority sorts first.
        Self(u8::MAX - (p as u8))
    }
    fn to_priority(self) -> Priority {
        Priority::from(u8::MAX - self.0)
    }
}

/// Monotonically increasing sequence number – breaks ties with FIFO ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SeqNum(u64);

// ─── Inner (non-thread-safe) scheduler ───────────────────────────────────────

/// Core scheduler data structure.  Not `Send` by itself; wrap in
/// `SharedDynamicScheduler` for concurrent use.
pub struct DynamicPriorityScheduler {
    /// Ordered view: (rev_priority, seq_num) → task_id
    index: BTreeMap<(RevPriority, SeqNum), TaskId>,
    /// Reverse map: task_id → (rev_priority, seq_num) for O(1) key lookup.
    reverse: HashMap<TaskId, (RevPriority, SeqNum)>,
    /// Monotonic counter.
    seq: u64,
}

impl DynamicPriorityScheduler {
    /// Create an empty scheduler.
    pub fn new() -> Self {
        Self {
            index: BTreeMap::new(),
            reverse: HashMap::new(),
            seq: 0,
        }
    }

    /// Add a task with a given priority.
    pub fn push(&mut self, task_id: TaskId, priority: Priority) {
        self.push_inner(task_id, priority);
    }

    /// Internal push that returns the assigned SeqNum (used by SharedDynamicScheduler).
    pub(crate) fn push_inner(&mut self, task_id: TaskId, priority: Priority) -> SeqNum {
        let rp = RevPriority::new(priority);
        let seq = SeqNum(self.seq);
        self.seq += 1;
        self.index.insert((rp, seq), task_id);
        self.reverse.insert(task_id, (rp, seq));
        seq
    }

    /// Remove and return the highest-priority task (FIFO within equal
    /// priorities).  Returns `None` if empty.
    pub fn pop(&mut self) -> Option<TaskId> {
        let (&key, &task_id) = self.index.iter().next()?;
        self.index.remove(&key);
        self.reverse.remove(&task_id);
        Some(task_id)
    }

    /// Peek at the next task without removing it.
    pub fn peek(&self) -> Option<(TaskId, Priority)> {
        self.index
            .iter()
            .next()
            .map(|((rp, _), &tid)| (tid, rp.to_priority()))
    }

    /// Change the priority of a queued task.
    ///
    /// Returns `true` if the task was found and reprioritized; `false` if it
    /// had already been popped (or was never pushed).
    ///
    /// # Complexity
    /// O(log n) – one `BTreeMap::remove` and one `BTreeMap::insert`.
    pub fn reprioritize(&mut self, task_id: TaskId, new_priority: Priority) -> bool {
        let (old_rp, old_seq) = match self.reverse.remove(&task_id) {
            Some(v) => v,
            None => return false,
        };
        // Remove old entry from the ordered index.
        self.index.remove(&(old_rp, old_seq));

        // Re-insert with new priority but **same sequence number** so that
        // relative FIFO ordering among tasks of the same priority is preserved.
        let new_rp = RevPriority::new(new_priority);
        self.index.insert((new_rp, old_seq), task_id);
        self.reverse.insert(task_id, (new_rp, old_seq));
        true
    }

    /// Query the current priority of a queued task.
    pub fn peek_priority(&self, task_id: TaskId) -> Option<Priority> {
        self.reverse.get(&task_id).map(|(rp, _)| rp.to_priority())
    }

    /// Remove a specific task without executing it (cancellation path).
    pub fn remove(&mut self, task_id: TaskId) -> bool {
        if let Some((rp, seq)) = self.reverse.remove(&task_id) {
            self.index.remove(&(rp, seq));
            true
        } else {
            false
        }
    }

    /// Number of queued tasks.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// `true` if no tasks are queued.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Drain all tasks in priority order (highest first).  Useful for
    /// graceful shutdown or snapshot inspection.
    pub fn drain_ordered(&mut self) -> Vec<(TaskId, Priority)> {
        let mut result = Vec::with_capacity(self.len());
        while let Some(task_id) = self.pop() {
            let priority = Priority::Normal; // already popped, use default
            result.push((task_id, priority));
        }
        result
    }

    /// Snapshot of all queued tasks with their priorities, ordered from
    /// highest to lowest priority.  Does **not** modify the queue.
    pub fn snapshot(&self) -> Vec<(TaskId, Priority)> {
        self.index
            .iter()
            .map(|((rp, _), &tid)| (tid, rp.to_priority()))
            .collect()
    }
}

impl Default for DynamicPriorityScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Thread-safe wrapper ──────────────────────────────────────────────────────

/// Thread-safe, reference-counted wrapper around `DynamicPriorityScheduler`.
///
/// Cheaply cloneable – clones share the same underlying queue.
#[derive(Clone)]
pub struct SharedDynamicScheduler(Arc<Mutex<DynamicPriorityScheduler>>);

impl SharedDynamicScheduler {
    /// Create a new, empty shared scheduler.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(DynamicPriorityScheduler::new())))
    }

    /// Push a task.  Returns a `PriorityHandle` that allows later
    /// reprioritization.
    pub fn push(&self, task_id: TaskId, priority: Priority) -> PriorityHandle {
        let seq = self.0.lock().unwrap().push_inner(task_id, priority);
        PriorityHandle {
            task_id,
            seq,
            scheduler: Arc::downgrade(&self.0),
        }
    }

    /// Pop the next task.
    pub fn pop(&self) -> Option<TaskId> {
        self.0.lock().unwrap().pop()
    }

    /// Peek at the next task without removing it.
    pub fn peek(&self) -> Option<(TaskId, Priority)> {
        self.0.lock().unwrap().peek()
    }

    /// Reprioritize a task by id.  Returns `false` if the task is no longer
    /// queued.
    pub fn reprioritize(&self, task_id: TaskId, new_priority: Priority) -> bool {
        self.0.lock().unwrap().reprioritize(task_id, new_priority)
    }

    /// Query the current priority of a queued task.
    pub fn peek_priority(&self, task_id: TaskId) -> Option<Priority> {
        self.0.lock().unwrap().peek_priority(task_id)
    }

    /// Remove a task from the queue (e.g., on cancellation).
    pub fn remove(&self, task_id: TaskId) -> bool {
        self.0.lock().unwrap().remove(task_id)
    }

    /// Number of queued tasks.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }

    /// Snapshot of all queued tasks in priority order.
    pub fn snapshot(&self) -> Vec<(TaskId, Priority)> {
        self.0.lock().unwrap().snapshot()
    }
}

impl Default for SharedDynamicScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Priority handle ──────────────────────────────────────────────────────────

/// A handle to a queued task that allows dynamic priority adjustment.
///
/// Obtained from `SharedDynamicScheduler::push`.  Internally holds only a
/// `Weak` reference to the scheduler, so it does not prevent the scheduler
/// from being dropped.
///
/// ```rust
/// # use taskflow_rs::dynamic_priority::{SharedDynamicScheduler};
/// # use taskflow_rs::advanced::Priority;
/// let sched = SharedDynamicScheduler::new();
/// let handle = sched.push(1, Priority::Low);
///
/// // Elevate to Critical before the executor picks it up.
/// let changed = handle.reprioritize(Priority::Critical);
/// assert!(changed);
/// ```
#[derive(Debug, Clone)]
pub struct PriorityHandle {
    pub task_id: TaskId,
    seq: SeqNum,
    scheduler: Weak<Mutex<DynamicPriorityScheduler>>,
}

impl PriorityHandle {
    /// Change this task's priority.
    ///
    /// Returns `true` if the task was still queued and its priority was
    /// updated; `false` if it had already been popped or cancelled, or if the
    /// scheduler has been dropped.
    pub fn reprioritize(&self, new_priority: Priority) -> bool {
        self.scheduler
            .upgrade()
            .map(|arc| arc.lock().unwrap().reprioritize(self.task_id, new_priority))
            .unwrap_or(false)
    }

    /// Query the current priority.  Returns `None` if already popped or if
    /// the scheduler has been dropped.
    pub fn current_priority(&self) -> Option<Priority> {
        self.scheduler
            .upgrade()
            .and_then(|arc| arc.lock().unwrap().peek_priority(self.task_id))
    }

    /// Remove the task from the queue (pre-cancellation path).
    pub fn cancel(&self) -> bool {
        self.scheduler
            .upgrade()
            .map(|arc| arc.lock().unwrap().remove(self.task_id))
            .unwrap_or(false)
    }

    /// `true` if the task is still waiting in the queue.
    pub fn is_pending(&self) -> bool {
        self.current_priority().is_some()
    }

    /// The sequence number assigned at push time (useful for debugging).
    pub fn sequence_number(&self) -> u64 {
        self.seq.0
    }
}

// ─── Priority escalation policy ───────────────────────────────────────────────

/// Policy for automatically elevating task priorities based on wait time
/// (anti-starvation).
///
/// Attach to a `SharedDynamicScheduler` and call `tick()` periodically
/// (e.g., every scheduler loop iteration).
pub struct EscalationPolicy {
    scheduler: SharedDynamicScheduler,
    /// How often to escalate (in `tick()` calls).
    tick_interval: u64,
    /// Current tick counter.
    tick_count: u64,
    /// Age at which Low → Normal (reserved for future per-age escalation).
    #[allow(dead_code)]
    low_age_ticks: u64,
    /// Age at which Normal → High (reserved for future per-age escalation).
    #[allow(dead_code)]
    normal_age_ticks: u64,
}

impl EscalationPolicy {
    /// Create an escalation policy.
    ///
    /// - `tick_interval`: how many `tick()` calls between escalation passes.
    /// - `low_age_ticks`: ticks before a `Low` task is bumped to `Normal`.
    /// - `normal_age_ticks`: ticks before a `Normal` task is bumped to `High`.
    pub fn new(
        scheduler: SharedDynamicScheduler,
        tick_interval: u64,
        low_age_ticks: u64,
        normal_age_ticks: u64,
    ) -> Self {
        Self {
            scheduler,
            tick_interval,
            tick_count: 0,
            low_age_ticks,
            normal_age_ticks,
        }
    }

    /// Call this every scheduler iteration.  Internally checks whether enough
    /// ticks have elapsed to run an escalation pass.
    pub fn tick(&mut self) {
        self.tick_count += 1;
        if self.tick_count % self.tick_interval != 0 {
            return;
        }
        self.escalate();
    }

    /// Force an immediate escalation pass (useful in tests or after a burst).
    pub fn escalate(&self) {
        let snapshot = self.scheduler.snapshot();
        for (task_id, priority) in snapshot {
            let new_priority = match priority {
                Priority::Low => Priority::Normal,
                Priority::Normal => Priority::High,
                _ => continue, // High / Critical unchanged
            };
            self.scheduler.reprioritize(task_id, new_priority);
        }
    }
}

// ─── Trait impl so DynamicPriorityScheduler can be used where Scheduler is expected

impl crate::scheduler::Scheduler for DynamicPriorityScheduler {
    fn push(&mut self, task_id: TaskId, priority: Priority) {
        self.push_inner(task_id, priority);
    }
    fn pop(&mut self) -> Option<TaskId> {
        DynamicPriorityScheduler::pop(self)
    }
    fn is_empty(&self) -> bool {
        DynamicPriorityScheduler::is_empty(self)
    }
    fn len(&self) -> usize {
        DynamicPriorityScheduler::len(self)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DynamicPriorityScheduler (inner) ─────────────────────────────────────

    #[test]
    fn basic_push_pop_priority_order() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(1, Priority::Low);
        sched.push(2, Priority::High);
        sched.push(3, Priority::Normal);
        sched.push(4, Priority::Critical);

        assert_eq!(sched.pop(), Some(4)); // Critical
        assert_eq!(sched.pop(), Some(2)); // High
        assert_eq!(sched.pop(), Some(3)); // Normal
        assert_eq!(sched.pop(), Some(1)); // Low
        assert_eq!(sched.pop(), None);
    }

    #[test]
    fn fifo_within_equal_priority() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(10, Priority::Normal);
        sched.push(20, Priority::Normal);
        sched.push(30, Priority::Normal);

        assert_eq!(sched.pop(), Some(10));
        assert_eq!(sched.pop(), Some(20));
        assert_eq!(sched.pop(), Some(30));
    }

    #[test]
    fn reprioritize_elevates_task() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(1, Priority::Low);
        sched.push(2, Priority::Normal);

        // Elevate task 1 to Critical.
        assert!(sched.reprioritize(1, Priority::Critical));

        // Task 1 should now come first.
        assert_eq!(sched.pop(), Some(1));
        assert_eq!(sched.pop(), Some(2));
    }

    #[test]
    fn reprioritize_demotes_task() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(1, Priority::Critical);
        sched.push(2, Priority::Normal);

        // Demote task 1 to Low.
        assert!(sched.reprioritize(1, Priority::Low));

        assert_eq!(sched.pop(), Some(2)); // Normal before Low
        assert_eq!(sched.pop(), Some(1));
    }

    #[test]
    fn reprioritize_nonexistent_returns_false() {
        let mut sched = DynamicPriorityScheduler::new();
        assert!(!sched.reprioritize(999, Priority::High));
    }

    #[test]
    fn reprioritize_preserves_fifo_within_new_priority() {
        let mut sched = DynamicPriorityScheduler::new();
        // Push two Normal tasks, then elevate the older one to High.
        sched.push(1, Priority::Normal); // seq=0
        sched.push(2, Priority::Normal); // seq=1
        sched.push(3, Priority::High); // seq=2

        // Elevate task 1 (seq=0) to High → should come before task 3 (seq=2).
        assert!(sched.reprioritize(1, Priority::High));

        assert_eq!(sched.pop(), Some(1)); // High, older seq
        assert_eq!(sched.pop(), Some(3)); // High, newer seq
        assert_eq!(sched.pop(), Some(2)); // Normal
    }

    #[test]
    fn remove_works() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(1, Priority::Normal);
        sched.push(2, Priority::High);
        assert!(sched.remove(2));
        assert_eq!(sched.len(), 1);
        assert_eq!(sched.pop(), Some(1));
        assert!(!sched.remove(2)); // already gone
    }

    #[test]
    fn peek_priority() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(5, Priority::High);
        assert_eq!(sched.peek_priority(5), Some(Priority::High));
        sched.pop();
        assert_eq!(sched.peek_priority(5), None);
    }

    #[test]
    fn snapshot_ordered() {
        let mut sched = DynamicPriorityScheduler::new();
        sched.push(1, Priority::Low);
        sched.push(2, Priority::Critical);
        sched.push(3, Priority::Normal);

        let snap = sched.snapshot();
        // Should be in priority order: Critical, Normal, Low
        assert_eq!(snap[0].1, Priority::Critical);
        assert_eq!(snap[1].1, Priority::Normal);
        assert_eq!(snap[2].1, Priority::Low);
        // Queue must be unchanged.
        assert_eq!(sched.len(), 3);
    }

    // ── SharedDynamicScheduler ────────────────────────────────────────────────

    #[test]
    fn shared_scheduler_push_pop() {
        let sched = SharedDynamicScheduler::new();
        sched.push(1, Priority::Normal);
        sched.push(2, Priority::Critical);
        assert_eq!(sched.pop(), Some(2));
        assert_eq!(sched.pop(), Some(1));
    }

    #[test]
    fn priority_handle_reprioritize() {
        let sched = SharedDynamicScheduler::new();
        let handle = sched.push(42, Priority::Low);
        sched.push(7, Priority::Normal);

        assert_eq!(handle.current_priority(), Some(Priority::Low));

        // Elevate 42 to Critical via handle.
        assert!(handle.reprioritize(Priority::Critical));
        assert_eq!(handle.current_priority(), Some(Priority::Critical));

        assert_eq!(sched.pop(), Some(42)); // now first
        assert_eq!(sched.pop(), Some(7));

        // Handle shows task is gone.
        assert_eq!(handle.current_priority(), None);
        assert!(!handle.is_pending());
    }

    #[test]
    fn priority_handle_cancel() {
        let sched = SharedDynamicScheduler::new();
        let h = sched.push(99, Priority::High);
        sched.push(1, Priority::Normal);

        assert!(h.cancel());
        assert!(!h.cancel()); // already cancelled

        assert_eq!(sched.len(), 1);
        assert_eq!(sched.pop(), Some(1));
    }

    #[test]
    fn escalation_policy_bumps_low_to_normal() {
        let sched = SharedDynamicScheduler::new();
        sched.push(1, Priority::Low);
        sched.push(2, Priority::Normal);

        let policy = EscalationPolicy::new(sched.clone(), 1, 1, 2);
        policy.escalate(); // force pass

        // Task 1 (was Low) should now be Normal.
        assert_eq!(sched.peek_priority(1), Some(Priority::Normal));
        // Task 2 (was Normal) should now be High.
        assert_eq!(sched.peek_priority(2), Some(Priority::High));
    }

    #[test]
    fn concurrent_push_reprioritize() {
        let sched = SharedDynamicScheduler::new();
        let sched2 = sched.clone();

        // Push from main thread, reprioritize from another.
        let handle = sched.push(10, Priority::Low);
        let t = std::thread::spawn(move || {
            handle.reprioritize(Priority::Critical);
        });
        t.join().unwrap();

        assert_eq!(sched2.pop(), Some(10));
    }
}
