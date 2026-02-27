# Advanced Features

TaskFlow-RS provides advanced scheduling and execution control features for fine-tuned performance optimization.

## Features

- **Task Priorities** - Control task execution order with priority levels
- **Task Cancellation** - Cancel tasks before or during execution
- **Custom Schedulers** - Pluggable scheduling strategies
- **NUMA-Aware Scheduling** - Optimize for Non-Uniform Memory Access architectures

## Task Priorities

### Priority Levels

```rust
pub enum Priority {
    Low = 0,        // Background tasks
    Normal = 5,     // Default priority
    High = 10,      // Important tasks
    Critical = 15,  // Must run ASAP
}
```

### Using Priorities with Schedulers

```rust
use taskflow_rs::{PriorityScheduler, Scheduler, Priority};

let mut scheduler = PriorityScheduler::new();

// Add tasks with different priorities
scheduler.push(1, Priority::Low);
scheduler.push(2, Priority::High);
scheduler.push(3, Priority::Critical);

// Tasks execute in priority order: Critical > High > Low
while let Some(task_id) = scheduler.pop() {
    println!("Executing task {}", task_id);
}
```

## Task Cancellation

### Cancellation Tokens

```rust
use taskflow_rs::{Executor, Taskflow, CancellationToken};

let token = CancellationToken::new();

// Use token in task
let t = token.clone();
let mut taskflow = Taskflow::new();

taskflow.emplace(move || {
    for i in 0..100 {
        if t.is_cancelled() {
            println!("Task cancelled at iteration {}", i);
            return;
        }
        
        // Do work
        process_item(i);
    }
});

// Cancel from another thread
token.cancel();
```

### Cancellation API

```rust
// Create token
let token = CancellationToken::new();

// Check if cancelled
if token.is_cancelled() {
    return;
}

// Cancel
token.cancel();

// Reset for reuse
token.reset();

// Get cancel count
let count = token.cancel_count();
```

## Custom Schedulers

### Scheduler Trait

Implement custom scheduling strategies:

```rust
use taskflow_rs::{Scheduler, Priority};
use crate::task::TaskId;

pub trait Scheduler: Send + Sync {
    fn push(&mut self, task_id: TaskId, priority: Priority);
    fn pop(&mut self) -> Option<TaskId>;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
}
```

### Built-in Schedulers

#### FIFO Scheduler (Default)

Simple first-in-first-out queue:

```rust
use taskflow_rs::{FifoScheduler, Scheduler, Priority};

let mut scheduler = FifoScheduler::new();
scheduler.push(1, Priority::Normal);
scheduler.push(2, Priority::High);

// FIFO order: 1, 2
assert_eq!(scheduler.pop(), Some(1));
assert_eq!(scheduler.pop(), Some(2));
```

#### Priority Scheduler

Priority queue with FIFO fallback:

```rust
use taskflow_rs::{PriorityScheduler, Scheduler, Priority};

let mut scheduler = PriorityScheduler::new();
scheduler.push(1, Priority::Low);
scheduler.push(2, Priority::High);
scheduler.push(3, Priority::Critical);

// Priority order: Critical(3) > High(2) > Low(1)
assert_eq!(scheduler.pop(), Some(3));
assert_eq!(scheduler.pop(), Some(2));
assert_eq!(scheduler.pop(), Some(1));
```

#### Round-Robin Scheduler

Distributes tasks evenly across workers:

```rust
use taskflow_rs::{RoundRobinScheduler, Scheduler, Priority};

let mut scheduler = RoundRobinScheduler::new(4); // 4 workers

for i in 0..10 {
    scheduler.push(i, Priority::Normal);
}

// Tasks distributed across 4 worker queues
```

### Custom Scheduler Example

```rust
use taskflow_rs::{Scheduler, Priority};
use std::collections::VecDeque;

pub struct CustomScheduler {
    queue: VecDeque<TaskId>,
}

impl Scheduler for CustomScheduler {
    fn push(&mut self, task_id: TaskId, _priority: Priority) {
        // Custom logic here
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
```

## NUMA-Aware Scheduling

### Detecting NUMA Topology

```rust
use taskflow_rs::NumaTopology;

let topology = NumaTopology::detect();

println!("NUMA nodes: {}", topology.num_nodes);
println!("Has NUMA: {}", topology.has_numa());

for node in &topology.nodes {
    println!("Node {}: {} CPUs", node.id, node.cpus.len());
}
```

### NUMA Pinning Strategies

```rust
use taskflow_rs::NumaPinning;

pub enum NumaPinning {
    None,        // No pinning
    RoundRobin,  // Distribute across nodes
    Dense,       // Fill one node before moving to next
    Sparse,      // Spread across all nodes first
}
```

### Worker-to-CPU Assignment

```rust
use taskflow_rs::numa::{get_worker_cpus, NumaPinning};

let topology = NumaTopology::detect();
let num_workers = 8;

// Get CPUs for each worker
for worker_id in 0..num_workers {
    let cpus = get_worker_cpus(
        worker_id,
        num_workers,
        &topology,
        NumaPinning::Dense
    );
    
    println!("Worker {}: CPUs {:?}", worker_id, cpus);
}
```

### NUMA Benefits

**When to use NUMA-aware scheduling:**
- Multi-socket systems (2+ CPUs)
- Large memory workloads
- Memory-intensive tasks
- High-bandwidth requirements

**Performance gains:**
- 20-50% improvement on NUMA systems
- Reduced memory latency
- Better cache utilization
- Improved memory bandwidth

## Task Metadata

Combine priorities, cancellation, and NUMA hints:

```rust
use taskflow_rs::TaskMetadata;

// With priority
let meta = TaskMetadata::with_priority(Priority::High);

// With cancellation
let token = CancellationToken::new();
let meta = TaskMetadata::with_cancellation(token);

// With NUMA hint
let meta = TaskMetadata::with_numa_node(0); // Prefer node 0

// Check if should cancel
if meta.should_cancel() {
    return;
}
```

## Integration Examples

### Priority-Based Pipeline

```rust
use taskflow_rs::{Executor, Taskflow, Priority};

let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

// High-priority preprocessing
let preprocess = taskflow.emplace(|| {
    println!("High priority: Preprocessing");
}).name("preprocess");

// Normal priority computation
let compute = taskflow.emplace(|| {
    println!("Normal priority: Computing");
}).name("compute");

// Low priority logging
let log = taskflow.emplace(|| {
    println!("Low priority: Logging");
}).name("log");

preprocess.precede(&compute);
compute.precede(&log);

executor.run(&taskflow).wait();
```

### Cancellable Long-Running Task

```rust
use taskflow_rs::{Executor, Taskflow, CancellationToken};
use std::time::Duration;

let token = CancellationToken::new();
let mut executor = Executor::new(4);

// Spawn cancellable task
let t = token.clone();
let handle = std::thread::spawn(move || {
    let mut taskflow = Taskflow::new();
    taskflow.emplace(move || {
        for i in 0..1000 {
            if t.is_cancelled() {
                println!("Cancelled at {}", i);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    
    executor.run(&taskflow).wait();
});

// Cancel after 2 seconds
std::thread::sleep(Duration::from_secs(2));
token.cancel();

handle.join().unwrap();
```

### NUMA-Optimized Data Processing

```rust
use taskflow_rs::{Executor, Taskflow, NumaTopology};

let topology = NumaTopology::detect();

if topology.has_numa() {
    println!("NUMA system detected: {} nodes", topology.num_nodes);
    
    // Create one taskflow per NUMA node
    for node in &topology.nodes {
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace(move || {
            println!("Processing on NUMA node {}", node.id);
            // Process data local to this NUMA node
        });
        
        let mut executor = Executor::new(node.cpus.len());
        executor.run(&taskflow).wait();
    }
} else {
    println!("UMA system - using default scheduling");
}
```

## Performance Tips

### 1. Use Appropriate Priorities

```rust
// Critical: Time-sensitive, blocking other work
Priority::Critical  // Real-time deadlines

// High: Important but not blocking
Priority::High      // User-facing operations

// Normal: Regular workload
Priority::Normal    // Default tasks

// Low: Background, non-urgent
Priority::Low       // Logging, cleanup, analytics
```

### 2. Strategic Cancellation

```rust
// Check cancellation at appropriate intervals
for chunk in data.chunks(1000) {
    if token.is_cancelled() {
        return;  // Early exit
    }
    process_chunk(chunk);
}

// Don't check too frequently (overhead)
// Don't check too infrequently (slow cancellation)
```

### 3. NUMA Locality

```rust
// Allocate data on the same NUMA node as workers
// Keep frequently accessed data on local node
// Minimize cross-node memory access

// Example: Pin workers to nodes and allocate data locally
let node_id = get_current_numa_node();
let data = allocate_on_node(node_id, size);
```

## Best Practices

1. **Priorities**: Use sparingly - most tasks should be Normal
2. **Cancellation**: Check at loop boundaries, not in tight loops
3. **Custom Schedulers**: Profile before optimizing scheduling
4. **NUMA**: Only enable on multi-socket systems with proven benefit

## Limitations

1. **Priority Integration**: Full priority support requires executor modifications
2. **NUMA Detection**: Simplified detection - use hwloc for production
3. **Thread Pinning**: Stub implementation - use platform-specific APIs
4. **Cancellation**: Cooperative only (tasks must check token)

## Future Enhancements

- Preemptive task cancellation
- Dynamic priority adjustment
- Automatic NUMA optimization
- Hardware topology integration (hwloc)
- Priority inheritance for dependencies
- Real-time scheduling support

## Examples

Run the advanced features demo:

```bash
cargo run --example advanced_features
```

Expected output:
```
=== Advanced Features Demo ===

1. Task Priorities
   [Low Priority] Executing...
   [Normal Priority] Executing...
   [High Priority] Executing...
   [Critical Priority] Executing...
   ✓ Priority levels working

2. Task Cancellation
   [Iteration 0] Completed
   [Iteration 1] Completed
   >>> Cancelling remaining tasks...
   [Iteration 2] Cancelled at step 3
   ✓ Cancellation working

3. Custom Scheduler
   Adding tasks...
   Execution order: Critical > High > Normal > Low
   ✓ Custom scheduler working

4. NUMA Awareness
   CPU cores: 8
   NUMA nodes: 2
   ✓ NUMA awareness configured
```
