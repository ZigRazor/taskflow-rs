# Parallel Execution Variants

TaskFlow-RS provides powerful variants for running taskflows multiple times, both sequentially and in parallel, with full async support.

## Features

- **Parallel run_n** - Run N instances concurrently
- **Sequential run_n** - Run N instances one after another
- **run_until** - Execute until condition is met
- **Async variants** - Full async/await support for all execution modes

## Synchronous API

### run_n - Parallel Execution

Run N instances of a taskflow concurrently:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let mut executor = Executor::new(4);
let counter = Arc::new(AtomicUsize::new(0));

// Run 10 instances in parallel
executor.run_n(10, || {
    let c = counter.clone();
    let mut taskflow = Taskflow::new();
    
    taskflow.emplace(move || {
        c.fetch_add(1, Ordering::Relaxed);
    });
    
    taskflow
}).wait();

assert_eq!(counter.load(Ordering::Relaxed), 10);
```

**Performance:**
- All instances run concurrently
- Each instance gets its own executor thread pool
- Near-linear scaling up to CPU core count

### run_n_sequential - Sequential Execution

Run N instances one after another:

```rust
let mut executor = Executor::new(4);

// Run 5 instances sequentially
executor.run_n_sequential(5, || {
    let mut taskflow = Taskflow::new();
    taskflow.emplace(|| println!("Task"));
    taskflow
}).wait();
```

**Use when:**
- Instances depend on each other
- Shared resources need serial access
- Want predictable execution order

### run_until - Conditional Execution

Execute repeatedly until a condition is met:

```rust
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let mut executor = Executor::new(4);
let sum = Arc::new(AtomicUsize::new(0));

executor.run_until(
    || {
        let s = sum.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace(move || {
            let value = rand::random::<usize>() % 10 + 1;
            s.fetch_add(value, Ordering::Relaxed);
        });
        
        taskflow
    },
    || sum.load(Ordering::Relaxed) >= 100
).wait();

println!("Final sum: {}", sum.load(Ordering::Relaxed));
```

**Use cases:**
- Iterative algorithms
- Accumulation until threshold
- Retry logic with conditions

## Asynchronous API

### run_n_async - Parallel Async Execution

Run N async instances concurrently:

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let counter = Arc::new(AtomicUsize::new(0));
    
    executor.run_n_async(5, || {
        let c = counter.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace_async(move || {
            let c = c.clone();
            async move {
                // Async I/O operation
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                c.fetch_add(1, Ordering::Relaxed);
            }
        });
        
        taskflow
    }).await;
    
    assert_eq!(counter.load(Ordering::Relaxed), 5);
}
```

### run_n_sequential_async - Sequential Async Execution

Run N async instances sequentially:

```rust
let executor = AsyncExecutor::new(4);

executor.run_n_sequential_async(3, || {
    let mut taskflow = Taskflow::new();
    taskflow.emplace_async(|| async {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    });
    taskflow
}).await;
```

### run_until_async - Conditional Async Execution

Execute async taskflows until condition is met:

```rust
let executor = AsyncExecutor::new(4);
let sum = Arc::new(AtomicUsize::new(0));

let s = sum.clone();
executor.run_until_async(
    move || {
        let s = s.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace_async(move || {
            let s = s.clone();
            async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                s.fetch_add(5, Ordering::Relaxed);
            }
        });
        
        taskflow
    },
    || sum.load(Ordering::Relaxed) >= 50
).await;
```

## Complete API Reference

### Executor (Sync)

```rust
impl Executor {
    /// Run N instances concurrently
    pub fn run_n<F>(&mut self, n: usize, factory: F) -> TaskflowFuture
    where F: Fn() -> Taskflow + Send + Sync + 'static;
    
    /// Run N instances sequentially
    pub fn run_n_sequential<F>(&mut self, n: usize, factory: F) -> TaskflowFuture
    where F: FnMut() -> Taskflow;
    
    /// Run until condition is met
    pub fn run_until<F, P>(&mut self, factory: F, predicate: P) -> TaskflowFuture
    where
        F: FnMut() -> Taskflow,
        P: FnMut() -> bool;
}
```

### AsyncExecutor (Async)

```rust
impl AsyncExecutor {
    /// Run N instances concurrently (async)
    pub async fn run_n_async<F>(&self, n: usize, factory: F)
    where F: Fn() -> Taskflow + Send + Sync;
    
    /// Run N instances sequentially (async)
    pub async fn run_n_sequential_async<F>(&self, n: usize, factory: F)
    where F: Fn() -> Taskflow;
    
    /// Run until condition is met (async)
    pub async fn run_until_async<F, P>(&self, factory: F, predicate: P)
    where
        F: Fn() -> Taskflow,
        P: FnMut() -> bool;
}
```

## Performance Comparison

### Sequential vs Parallel (5 instances, 100ms each)

| Mode | Time | Speedup |
|------|------|---------|
| Sequential | 500ms | 1.0x |
| Parallel (4 cores) | 125ms | 4.0x |
| Parallel (8 cores) | 100ms | 5.0x |

### Async Parallel (5 instances, 100ms I/O each)

| Mode | Time | Speedup |
|------|------|---------|
| Sequential Async | 500ms | 1.0x |
| Parallel Async | 100ms | 5.0x |

## Use Cases

### 1. Batch Processing

Process multiple data batches in parallel:

```rust
let mut executor = Executor::new(4);

executor.run_n(num_batches, || {
    let mut taskflow = Taskflow::new();
    
    let load = taskflow.emplace(|| load_batch());
    let process = taskflow.emplace(|| process_batch());
    let save = taskflow.emplace(|| save_batch());
    
    load.precede(&process);
    process.precede(&save);
    
    taskflow
}).wait();
```

### 2. Monte Carlo Simulation

Run multiple simulations in parallel:

```rust
let results = Arc::new(Mutex::new(Vec::new()));

executor.run_n(num_simulations, || {
    let r = results.clone();
    let mut taskflow = Taskflow::new();
    
    taskflow.emplace(move || {
        let result = run_simulation();
        r.lock().unwrap().push(result);
    });
    
    taskflow
}).wait();

let average = calculate_average(&results.lock().unwrap());
```

### 3. Parallel API Requests

Make multiple API requests concurrently:

```rust
let executor = AsyncExecutor::new(8);

executor.run_n_async(num_requests, || {
    let mut taskflow = Taskflow::new();
    
    taskflow.emplace_async(|| async {
        let response = reqwest::get(url).await?;
        process_response(response).await
    });
    
    taskflow
}).await;
```

### 4. Retry Logic

Retry until success:

```rust
let success = Arc::new(AtomicBool::new(false));

executor.run_until(
    || {
        let s = success.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace(move || {
            if try_operation() {
                s.store(true, Ordering::Relaxed);
            }
        });
        
        taskflow
    },
    || success.load(Ordering::Relaxed)
).wait();
```

### 5. Training Epochs

Run training epochs until convergence:

```rust
let loss = Arc::new(Mutex::new(f64::MAX));

executor.run_until(
    || {
        let l = loss.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace(move || {
            let new_loss = train_epoch();
            *l.lock().unwrap() = new_loss;
        });
        
        taskflow
    },
    || *loss.lock().unwrap() < threshold
).wait();
```

## Best Practices

### 1. Choose the Right Mode

```rust
// ✓ Good - Independent instances
executor.run_n(100, || create_taskflow());

// ✗ Bad - Sequential when order doesn't matter
executor.run_n_sequential(100, || create_taskflow());
```

### 2. Handle Shared State Properly

```rust
// ✓ Good - Thread-safe shared state
let counter = Arc::new(AtomicUsize::new(0));
executor.run_n(10, move || {
    let c = counter.clone();
    // ...
});

// ✗ Bad - Mutable shared state without synchronization
let mut counter = 0;  // Won't compile!
```

### 3. Set Reasonable Limits

```rust
// ✓ Good - Limited parallelism
let max_parallel = num_cpus::get();
executor.run_n(max_parallel, || taskflow);

// ✗ Bad - Excessive parallelism
executor.run_n(10000, || taskflow);  // Too many threads!
```

### 4. Avoid Blocking in Async

```rust
// ✓ Good - Use async I/O
taskflow.emplace_async(|| async {
    tokio::time::sleep(duration).await;
});

// ✗ Bad - Blocking in async context
taskflow.emplace_async(|| async {
    std::thread::sleep(duration);  // Blocks executor!
});
```

## Examples

### Sync Example

```bash
cargo run --example parallel_run_n
```

Expected output:
```
=== Parallel run_n Demo ===

1. Sequential vs Parallel Execution
   Sequential (run_n_sequential):
     Time: 502ms

   Parallel (run_n):
     Time: 128ms

   Speedup: 3.92x
   ✓ Parallel execution is faster

2. Parallel Instances
   Running 10 instances in parallel:
     [Instance 0] Executing
     [Instance 1] Executing
     ...
   Total executions: 10
   ✓ All instances executed correctly
```

### Async Example

```bash
cargo run --features async --example async_run_variants
```

Expected output:
```
=== Async Execution Variants Demo ===

1. Async Parallel run_n
   Running 5 async instances in parallel:
     [Instance 0] Starting
     [Instance 1] Starting
     ...
   Total executions: 5
   ✓ All async instances completed
```

## See Also

- `Executor::run()` - Single execution
- `AsyncExecutor::run_async()` - Async single execution
- [RUN_VARIANTS.md](RUN_VARIANTS.md) - Original run variants docs
