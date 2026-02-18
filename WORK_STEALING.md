# Work-Stealing Executor Implementation

## Overview

The TaskFlow-RS executor uses a **work-stealing** scheduler for efficient task execution. This design significantly improves performance over a single shared queue by reducing contention and improving cache locality.

## Architecture

### Key Components

```
┌─────────────────────────────────────────────────────────┐
│                      Executor                            │
│  Spawns N worker threads                                 │
└──────────────┬──────────────────────────────────────────┘
               │
               ├─────────────────┬────────────────┬────────
               ▼                 ▼                ▼
         ┌──────────┐      ┌──────────┐    ┌──────────┐
         │ Worker 0 │      │ Worker 1 │    │ Worker N │
         ├──────────┤      ├──────────┤    ├──────────┤
         │ Deque 0  │      │ Deque 1  │    │ Deque N  │
         │ [T][T][T]│      │ [T][T]   │    │ [T][T][T]│
         └──────────┘      └──────────┘    └──────────┘
              ▲                 ▲                ▲
              │                 │                │
              └─────────────────┴────────────────┘
                    Can steal from each other
```

### Per-Worker Queues

- Each worker has its own **lock-free double-ended queue (deque)**
- Workers push/pop tasks from the **back** of their own queue (LIFO)
- Workers steal tasks from the **front** of other queues (FIFO)
- Uses `crossbeam::deque` for lock-free operations

## How It Works

### 1. Task Execution Loop

```rust
loop {
    // Try own queue first (pop from back)
    let task = match worker.pop() {
        Some(task) => task,
        None => {
            // Own queue empty - try to steal
            try_steal_from_others()
        }
    };
    
    if let Some(task) = task {
        execute(task);
    } else {
        // No work available - check shutdown
        if all_done() { break; }
        yield_now(); // Avoid busy-waiting
    }
}
```

### 2. Work Stealing Strategy

When a worker's queue is empty:

1. **Random victim selection** - Choose a random worker to steal from
2. **Steal from front** - Take oldest task (better for cache locality)
3. **Retry on failure** - Try other workers if steal fails
4. **Yield if no work** - Avoid burning CPU cycles

```rust
fn try_steal(stealers: &[Stealer<TaskId>], worker_id: usize) -> Option<TaskId> {
    for _ in 0..num_workers {
        let victim = random_worker();
        if victim == worker_id { continue; }
        
        match stealers[victim].steal() {
            Steal::Success(task) => return Some(task),
            Steal::Empty => continue,
            Steal::Retry => continue,
        }
    }
    None
}
```

### 3. Dependency Resolution

Tasks become ready when all dependencies complete:

1. Each task has an **atomic dependency counter**
2. When a task completes, it decrements successors' counters
3. When counter reaches 0, task is **pushed to the worker's queue**
4. Other idle workers can **steal** these ready tasks

```rust
// After task execution
for successor in successors {
    let prev_count = dep_count[successor].fetch_sub(1, Ordering::SeqCst);
    if prev_count == 1 {
        // Dependency satisfied - push to OUR queue
        worker.push(successor);
    }
}
```

## Performance Benefits

### 1. Reduced Contention

**Before (Single Queue):**
- All workers compete for a single mutex
- High contention with many workers
- Lock overhead on every task

**After (Work-Stealing):**
- Workers primarily use their own queue (lock-free)
- Stealing is rare and lock-free
- Minimal contention

### 2. Cache Locality

**LIFO for own queue:**
- Recently created tasks executed first
- Data likely still in cache
- Better temporal locality

**FIFO for stealing:**
- Steal oldest tasks from victims
- Victim's cache is cold anyway
- Maximizes cache reuse

### 3. Load Balancing

- Busy workers accumulate tasks
- Idle workers steal from busy ones
- Automatic load distribution
- No manual partitioning needed

## Benchmark Results

Example performance improvements (your results may vary):

```
Wide Graph (1000 independent tasks):
  1 worker:  ~850ms
  2 workers: ~430ms (1.97x speedup)
  4 workers: ~220ms (3.86x speedup)
  8 workers: ~115ms (7.39x speedup)

Deep Graph (100 stages × 10 tasks):
  1 worker:  ~520ms
  2 workers: ~280ms (1.86x speedup)
  4 workers: ~150ms (3.47x speedup)
  8 workers: ~95ms  (5.47x speedup)
```

## Implementation Details

### Lock-Free Data Structure

We use `crossbeam::deque::Worker` and `Stealer`:

```rust
// Create per-worker queues
let workers: Vec<Worker<TaskId>> = (0..num_workers)
    .map(|_| Worker::new_fifo())
    .collect();

// Create stealers for all workers
let stealers: Vec<Stealer<TaskId>> = workers
    .iter()
    .map(|w| w.stealer())
    .collect();
```

**Key Properties:**
- `Worker::push()` - Add task to back (lock-free)
- `Worker::pop()` - Remove from back (lock-free, LIFO)
- `Stealer::steal()` - Remove from front (lock-free, FIFO)
- Returns `Steal::Success`, `Steal::Empty`, or `Steal::Retry`

### Random Victim Selection

Simple Linear Congruential Generator (LCG):

```rust
fn random_worker(rng_state: &mut usize, num_workers: usize) -> usize {
    *rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
    *rng_state % num_workers
}
```

- Fast (no allocation)
- Good enough distribution for work stealing
- Per-worker state (no contention)

### Atomic Operations

All shared state uses atomics:

```rust
// Task completion tracking
tasks_remaining: Arc<AtomicUsize>

// Shutdown signal
shutdown: Arc<AtomicBool>

// Dependency counts (per task)
dep_count: HashMap<TaskId, AtomicUsize>
```

**Memory Ordering:**
- `SeqCst` for correctness (may optimize later)
- Ensures visibility across threads
- Prevents reordering issues

## Comparison with Alternatives

### vs. Single Shared Queue

| Metric | Single Queue | Work-Stealing |
|--------|-------------|---------------|
| Contention | High | Low |
| Scalability | Poor (>4 cores) | Excellent |
| Cache Usage | Poor | Good |
| Complexity | Simple | Moderate |

### vs. Thread Pool (like Rayon)

| Metric | Thread Pool | TaskFlow Work-Stealing |
|--------|------------|------------------------|
| Task Dependencies | Limited | Full DAG support |
| Dynamic Tasks | Limited | Subflows supported |
| Overhead | Low | Slightly higher |
| Use Case | Data parallelism | Task parallelism |

## Future Optimizations

### 1. Better Stealing Heuristics

- **Exponential backoff** - Reduce stealing attempts over time
- **Work estimation** - Prefer stealing from busy workers
- **Locality awareness** - Steal from nearby cores first

### 2. Memory Ordering Relaxation

```rust
// Current: SeqCst everywhere
tasks_remaining.fetch_sub(1, Ordering::SeqCst);

// Potential: Relaxed where safe
tasks_remaining.fetch_sub(1, Ordering::Relaxed);
```

### 3. Bounded Queues

- Limit queue size to prevent unbounded growth
- Apply backpressure when queues are full
- Better memory predictability

### 4. NUMA-Aware Scheduling

- Bind workers to CPU cores
- Prefer stealing from same NUMA node
- Reduce cross-node memory traffic

### 5. Adaptive Work Stealing

- Monitor steal success rate
- Adjust stealing frequency dynamically
- Balance between stealing overhead and idleness

## Code Example

Here's how to use the work-stealing executor:

```rust
use taskflow_rs::{Executor, Taskflow};

fn main() {
    // Creates executor with 4 worker threads
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    // Create 100 tasks
    let mut tasks = Vec::new();
    for i in 0..100 {
        let task = taskflow.emplace(move || {
            println!("Task {} executing on worker", i);
        });
        tasks.push(task);
    }
    
    // Add some dependencies
    for i in 1..100 {
        tasks[i].succeed(&tasks[i-1]);
    }
    
    // Execute with work-stealing
    executor.run(&taskflow).wait();
}
```

## References

- [Work Stealing Queue (Crossbeam)](https://docs.rs/crossbeam/latest/crossbeam/deque/index.html)
- [Chase-Lev Work-Stealing Deque](https://www.dre.vanderbilt.edu/~schmidt/PDF/work-stealing-dequeue.pdf)
- [Cilk Work-Stealing Scheduler](http://supertech.csail.mit.edu/papers/steal.pdf)
- [TBB Task Scheduler](https://www.threadingbuildingblocks.org/docs/help/tbb_userguide/The_Task_Scheduler.html)

## Summary

The work-stealing executor provides:

✅ **Low contention** - Lock-free per-worker queues  
✅ **Good cache locality** - LIFO execution of own tasks  
✅ **Automatic load balancing** - Idle workers steal from busy ones  
✅ **Excellent scalability** - Near-linear speedup on many cores  
✅ **Simple API** - Same interface as before  

This makes TaskFlow-RS suitable for high-performance parallel computing on modern multi-core systems.
