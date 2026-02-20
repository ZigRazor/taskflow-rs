# Testing Async Support

This guide covers how to test the async task functionality in TaskFlow-RS.

## Compilation Tests

### Test 1: Build without async feature (default)
```bash
cargo build
cargo test
```

**Expected:** Compiles successfully. `Executor` works normally. No async functionality available.

### Test 2: Build with async feature
```bash
cargo build --features async
cargo test --features async
```

**Expected:** Compiles successfully. Both `Executor` and `AsyncExecutor` available.

### Test 3: Attempt to use async task with regular executor
```rust
let mut executor = Executor::new(4);  // Regular executor
let mut taskflow = Taskflow::new();

let task = taskflow.emplace_async(|| async {
    println!("This will panic!");
});

executor.run(&taskflow).wait();  // ❌ PANICS with helpful message
```

**Expected:** Panics with message: "Async tasks require AsyncExecutor. Use AsyncExecutor::new() instead of Executor::new()"

## Running Examples

### Example 1: Async Tasks Demo

The async_tasks example automatically requires the async feature (configured in Cargo.toml), so you can run it with:

```bash
cargo run --features async --example async_tasks
```

Or more simply (cargo will automatically enable required features):

```bash
cargo run --example async_tasks --features async
```

**Note:** If you try to run without the async feature, cargo will inform you that the example requires it.

**Expected Output:**
```
=== Async Tasks Demo ===

1. Basic Async Tasks
   Running async tasks with tokio

   [Task A] Starting async work...
   [Task A] Completed!
   [Task B] Starting async work...
   [Task C] Starting async work...
   [Task B] Completed!
   [Task C] Completed!
   ✓ All async tasks completed

2. Mixed Sync and Async Tasks
   Combining synchronous and asynchronous tasks

   [Sync] Processing synchronously
   [Sync] Counter = 1
   [Async] Starting async work
   [Async] Counter = 11
   [Final] Final counter = 11
   ✓ Mixed workflow completed

3. Simulated Async HTTP Requests
   Parallel async I/O operations

   [Request 0] Fetching data...
   [Request 1] Fetching data...
   [Request 2] Fetching data...
   [Request 3] Fetching data...
   [Request 4] Fetching data...
   [Request 0] Received: data_0
   [Request 1] Received: data_1
   [Request 2] Received: data_2
   [Request 3] Received: data_3
   [Request 4] Received: data_4
   ✓ Received 5 responses: ["data_0", "data_1", "data_2", "data_3", "data_4"]

4. Async Data Pipeline
   Multi-stage async processing

   [Fetch] Fetching raw data...
   [Fetch] Fetched 5 items
   [Transform] Transforming data...
   [Transform] Transformed 5 items
   [Filter] Filtering data...
   [Filter] Retained 3 items
   [Save] Saving results...
   [Save] Final data: [6, 8, 10]
   ✓ Pipeline completed
```

## Unit Tests

### Test Mixed Workflows
```rust
#[cfg(feature = "async")]
#[tokio::test]
async fn test_mixed_sync_async() {
    use taskflow_rs::{AsyncExecutor, Taskflow};
    
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let counter = std::sync::Arc::new(std::sync::Mutex::new(0));
    
    // Sync task
    let c1 = counter.clone();
    let sync = taskflow.emplace(move || {
        *c1.lock().unwrap() += 1;
    });
    
    // Async task
    let c2 = counter.clone();
    let async_task = taskflow.emplace_async(move || async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        *c2.lock().unwrap() += 10;
    });
    
    sync.precede(&async_task);
    
    executor.run(&taskflow);
    
    assert_eq!(*counter.lock().unwrap(), 11);
}
```

### Test Dependencies
```rust
#[cfg(feature = "async")]
#[tokio::test]
async fn test_async_dependencies() {
    use taskflow_rs::{AsyncExecutor, Taskflow};
    use std::sync::{Arc, Mutex};
    
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let order = Arc::new(Mutex::new(Vec::new()));
    
    let o1 = order.clone();
    let task_a = taskflow.emplace_async(move || async move {
        o1.lock().unwrap().push(1);
    });
    
    let o2 = order.clone();
    let task_b = taskflow.emplace_async(move || async move {
        o2.lock().unwrap().push(2);
    });
    
    task_a.precede(&task_b);
    
    executor.run(&taskflow);
    
    let order = order.lock().unwrap();
    assert_eq!(*order, vec![1, 2]);
}
```

## Performance Testing

### Async I/O Performance
```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::time::Instant;

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    
    // Sequential async
    let start = Instant::now();
    for _ in 0..10 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    let sequential_time = start.elapsed();
    println!("Sequential: {:?}", sequential_time);
    
    // Parallel async with TaskFlow
    let start = Instant::now();
    let mut taskflow = Taskflow::new();
    for _ in 0..10 {
        taskflow.emplace_async(|| async {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        });
    }
    executor.run(&taskflow);
    let parallel_time = start.elapsed();
    println!("Parallel: {:?}", parallel_time);
    
    // Note: Current implementation is sequential, so times will be similar
    // Future parallel async execution will show significant speedup
}
```

## Troubleshooting

### Issue: "Async tasks require AsyncExecutor"
**Cause:** Trying to use `emplace_async()` with regular `Executor`  
**Solution:** Use `AsyncExecutor::new()` instead of `Executor::new()`

### Issue: "cannot find `AsyncExecutor` in crate `taskflow_rs`"
**Cause:** `async` feature not enabled  
**Solution:** Add to Cargo.toml: `taskflow-rs = { features = ["async"] }`

### Issue: Tests hang indefinitely
**Cause:** Missing `#[tokio::test]` or `#[tokio::main]` attribute  
**Solution:** Ensure async tests use `#[tokio::test]` instead of `#[test]`

### Issue: Compilation errors with `Future` or `Pin`
**Cause:** Missing tokio dependency  
**Solution:** Add: `tokio = { version = "1", features = ["full"] }`

## Next Steps

Once async support is verified:
1. Test with real async I/O (HTTP requests, file operations)
2. Benchmark async vs sync performance
3. Test error handling in async tasks
4. Verify memory safety under concurrent async execution
5. Consider implementing parallel async execution (currently sequential)
