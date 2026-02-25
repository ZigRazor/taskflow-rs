# Run Variants

TaskFlow-RS provides flexible execution patterns for running taskflows multiple times, conditionally, or concurrently.

## Features

- **Run N Times** - Execute a taskflow a specific number of times
- **Run Until Condition** - Execute repeatedly until a predicate is satisfied
- **Run Many Concurrently** - Execute multiple taskflows in parallel
- **Batch Execution** - Convenient methods for common patterns

## API Overview

### Run N Times

Execute a taskflow N times sequentially using a factory function:

```rust
pub fn run_n<F>(&mut self, n: usize, factory: F) -> TaskflowFuture
where
    F: FnMut() -> Taskflow
```

**Why a factory function?** Rust's `FnOnce` closures can only execute once. To run a workflow multiple times, we need to create a fresh taskflow for each iteration.

**Example:**
```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let mut executor = Executor::new(4);
let counter = Arc::new(AtomicUsize::new(0));

let c = counter.clone();
executor.run_n(10, move || {
    let mut taskflow = Taskflow::new();
    let c = c.clone();
    
    taskflow.emplace(move || {
        let count = c.fetch_add(1, Ordering::Relaxed) + 1;
        println!("Iteration {}", count);
    });
    
    taskflow
}).wait();

assert_eq!(counter.load(Ordering::Relaxed), 10);
```

### Run Until Condition

Execute repeatedly until a predicate returns true:

```rust
pub fn run_until<F, P>(&mut self, factory: F, predicate: P) -> TaskflowFuture
where
    F: FnMut() -> Taskflow,
    P: FnMut() -> bool
```

**Example:**
```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let mut executor = Executor::new(4);
let sum = Arc::new(AtomicUsize::new(0));

let s = sum.clone();
let s_check = sum.clone();

executor.run_until(
    move || {
        let mut taskflow = Taskflow::new();
        let s = s.clone();
        taskflow.emplace(move || {
            s.fetch_add(5, Ordering::Relaxed);
        });
        taskflow
    },
    move || s_check.load(Ordering::Relaxed) >= 50
).wait();

// sum is now >= 50
```

### Run Many Concurrently

Execute multiple taskflows in parallel:

```rust
pub fn run_many(&mut self, taskflows: &[&Taskflow]) -> Vec<TaskflowFuture>
```

**Example:**
```rust
use taskflow_rs::{Executor, Taskflow};

let mut executor = Executor::new(4);

let flow1 = create_preprocessing_flow();
let flow2 = create_analysis_flow();
let flow3 = create_export_flow();

// Run all three concurrently
let futures = executor.run_many(&[&flow1, &flow2, &flow3]);

// Wait for all to complete
for future in futures {
    future.wait();
}
```

### Run Many and Wait (Convenience)

Run multiple taskflows and wait for all to complete:

```rust
pub fn run_many_and_wait(&mut self, taskflows: &[&Taskflow])
```

**Example:**
```rust
let mut executor = Executor::new(4);

let flows = vec![flow1, flow2, flow3, flow4, flow5];
let refs: Vec<_> = flows.iter().collect();

// Run all and wait
executor.run_many_and_wait(&refs);

println!("All flows completed!");
```

## Use Cases

### Use Case 1: Batch Processing

Process batches of data repeatedly:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

let mut executor = Executor::new(4);
let data = Arc::new(Mutex::new(Vec::new()));

let mut taskflow = Taskflow::new();
let d = data.clone();

taskflow.emplace(move || {
    // Fetch next batch
    let batch = fetch_batch();
    
    // Process batch
    let results = process_batch(batch);
    
    // Store results
    d.lock().unwrap().extend(results);
});

// Process 100 batches
executor.run_n(&taskflow, 100).wait();
```

### Use Case 2: Convergence Loop

Run until convergence in iterative algorithms:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

let mut executor = Executor::new(4);
let error = Arc::new(AtomicU64::new(u64::MAX));

let mut taskflow = Taskflow::new();
let e = error.clone();

taskflow.emplace(move || {
    let new_error = compute_iteration_error();
    
    // Store as u64 bit pattern
    e.store(new_error.to_bits(), Ordering::Relaxed);
});

let e = error.clone();
executor.run_until(&taskflow, move || {
    let err = f64::from_bits(e.load(Ordering::Relaxed));
    err < 0.001 // Converged when error < threshold
}).wait();
```

### Use Case 3: Parallel Pipelines

Run multiple independent pipelines concurrently:

```rust
use taskflow_rs::{Executor, Taskflow};

let mut executor = Executor::new(8);

// Create pipelines for different data sources
let pipeline_a = create_pipeline("source_a");
let pipeline_b = create_pipeline("source_b");
let pipeline_c = create_pipeline("source_c");

// Process all sources concurrently
executor.run_many_and_wait(&[&pipeline_a, &pipeline_b, &pipeline_c]);
```

### Use Case 4: Retry Logic

Retry failed operations with run_until:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

let mut executor = Executor::new(4);
let success = Arc::new(AtomicBool::new(false));

let mut taskflow = Taskflow::new();
let s = success.clone();

taskflow.emplace(move || {
    match try_operation() {
        Ok(_) => s.store(true, Ordering::Relaxed),
        Err(_) => {
            println!("Retry...");
            std::thread::sleep(Duration::from_secs(1));
        }
    }
});

let s = success.clone();
executor.run_until(&taskflow, move || {
    s.load(Ordering::Relaxed)
}).wait();
```

### Use Case 5: Training Epochs

Run training epochs in machine learning:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

let mut executor = Executor::new(4);
let model = Arc::new(Mutex::new(Model::new()));

let mut epoch_flow = Taskflow::new();

// Forward pass
let m1 = model.clone();
let forward = epoch_flow.emplace(move || {
    m1.lock().unwrap().forward_pass();
});

// Backward pass
let m2 = model.clone();
let backward = epoch_flow.emplace(move || {
    m2.lock().unwrap().backward_pass();
});

// Update weights
let m3 = model.clone();
let update = epoch_flow.emplace(move || {
    m3.lock().unwrap().update_weights();
});

forward.precede(&backward);
backward.precede(&update);

// Run 100 epochs
executor.run_n(&epoch_flow, 100).wait();
```

## Advanced Patterns

### Pattern 1: Conditional Iterations with State

```rust
struct State {
    iteration: usize,
    converged: bool,
}

let state = Arc::new(Mutex::new(State {
    iteration: 0,
    converged: false,
}));

let mut taskflow = Taskflow::new();
let s = state.clone();

taskflow.emplace(move || {
    let mut state = s.lock().unwrap();
    state.iteration += 1;
    
    // Check convergence
    if check_convergence() {
        state.converged = true;
    }
});

let s = state.clone();
executor.run_until(&taskflow, move || {
    let state = s.lock().unwrap();
    state.converged || state.iteration >= 1000
}).wait();
```

### Pattern 2: Pipeline with Feedback

```rust
let mut executor = Executor::new(4);
let needs_reprocessing = Arc::new(AtomicBool::new(false));

let mut taskflow = Taskflow::new();
let n = needs_reprocessing.clone();

taskflow.emplace(move || {
    let quality = process_data();
    n.store(quality < THRESHOLD, Ordering::Relaxed);
});

let n = needs_reprocessing.clone();
executor.run_until(&taskflow, move || {
    !n.load(Ordering::Relaxed)
}).wait();
```

### Pattern 3: Hierarchical Execution

```rust
// Run multiple workflows N times each
let flows = vec![flow1, flow2, flow3];

for flow in &flows {
    executor.run_n(flow, 5).wait();
}

// Or run them concurrently with different repetitions
let mut handles = Vec::new();

for (flow, n) in flows.iter().zip(vec![3, 5, 7]) {
    let handle = std::thread::spawn(move || {
        let mut executor = Executor::new(2);
        executor.run_n(flow, n).wait();
    });
    handles.push(handle);
}

for handle in handles {
    handle.join().unwrap();
}
```

## Performance Considerations

### Sequential vs Concurrent

**run_n** runs taskflows sequentially:
- Lower memory usage
- Predictable execution order
- Better for workflows with shared state

**run_many** runs taskflows concurrently:
- Higher throughput for independent workflows
- Better CPU utilization
- Requires more worker threads

### Optimal Worker Count

For `run_many`:
```rust
// If running N independent workflows concurrently
let workers_per_flow = 2;
let num_flows = 5;
let total_workers = workers_per_flow * num_flows;

let mut executor = Executor::new(total_workers);
```

### Memory Usage

Each concurrent taskflow requires:
- Task graph storage
- Worker thread stacks
- Intermediate results

Monitor memory when running many large workflows concurrently.

## Best Practices

### 1. Prefer run_many_and_wait for Simple Cases

```rust
// Good: Simple and clear
executor.run_many_and_wait(&flows);

// Less good: More verbose
let futures = executor.run_many(&flows);
for f in futures { f.wait(); }
```

### 2. Use Atomic Types for Counters

```rust
// Good: Lock-free counter
let counter = Arc::new(AtomicUsize::new(0));

// Less good: Mutex overhead for simple counter
let counter = Arc::new(Mutex::new(0));
```

### 3. Limit Iterations in run_until

```rust
// Good: Bounded iterations
let iterations = Arc::new(AtomicUsize::new(0));
let i = iterations.clone();

executor.run_until(&taskflow, move || {
    let count = i.fetch_add(1, Ordering::Relaxed);
    condition() || count >= MAX_ITERATIONS
}).wait();

// Avoid: Unbounded loop
executor.run_until(&taskflow, || {
    condition() // Could loop forever
}).wait();
```

### 4. Clone Arcs, Not Data

```rust
// Good: Share via Arc
let data = Arc::new(expensive_data);
let d1 = data.clone(); // Cheap Arc clone
let d2 = data.clone();

// Avoid: Clone expensive data
let data1 = expensive_data.clone(); // Expensive
let data2 = expensive_data.clone();
```

## Limitations

1. **Sequential run_n**: Runs are sequential, not parallel
2. **Predicate Evaluation**: run_until checks predicate after each run, not during
3. **No Early Exit**: run_n always runs N times (use run_until for early exit)
4. **Shared State**: Be careful with mutable shared state across runs

## Examples

Run the examples:

```bash
# Run variants demo
cargo run --example run_variants
```

Expected output:
```
=== Run Variants Demo ===

1. Run N Times
   Executing workflow 5 times
   [Iteration 1] Processing...
   [Iteration 2] Processing...
   ...
   ✓ Completed 5 iterations

2. Run Until Condition
   Running until sum reaches 50
   [Iteration 1] Added 1 (total: 1)
   [Iteration 2] Added 2 (total: 3)
   ...
   ✓ Stopped at sum = 55

3. Run Many Workflows
   Running 3 different workflows concurrently
   All workflows started...
   Workflow 1 completed
   Workflow 2 completed
   Workflow 3 completed
   ✓ All workflows completed
```

## Future Enhancements

- Parallel run_n (run N instances concurrently)
- Early termination for run_n
- Progress callbacks
- Async variants (run_n_async, run_until_async)
- Dynamic workflow scheduling
