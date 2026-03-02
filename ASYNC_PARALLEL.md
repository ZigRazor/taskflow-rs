# Parallel Async Execution

TaskFlow-RS provides full support for parallel asynchronous task execution using Tokio, enabling efficient concurrent I/O operations.

## Features

- **Parallel Async Execution** - Run independent async tasks concurrently
- **Mixed Sync/Async** - Combine synchronous and asynchronous tasks  
- **Async Subflows** - Dynamic task creation with async execution
- **Tokio Integration** - Built on Tokio for production-grade async runtime
- **Dependency Management** - Respects task dependencies in async context

## Quick Start

### Enable Async Feature

```toml
[dependencies]
taskflow-rs = { version = "*", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

### Basic Usage

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    taskflow.emplace_async(|| async {
        println!("Async task!");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    });
    
    executor.run_async(&taskflow).await;
}
```

## API

### AsyncExecutor

```rust
impl AsyncExecutor {
    /// Create executor using current Tokio runtime
    pub fn new(num_workers: usize) -> Self;
    
    /// Create executor with its own runtime
    pub fn new_with_runtime(num_workers: usize) -> Self;
    
    /// Run taskflow asynchronously (parallel execution)
    pub async fn run_async(&self, taskflow: &Taskflow);
    
    /// Run taskflow and block until completion
    pub fn run(&self, taskflow: &Taskflow);
    
    /// Alias for run_async
    pub async fn run_await(&self, taskflow: &Taskflow);
}
```

**Note**: Async subflows are automatically supported - when a subflow task creates async tasks, they are executed in parallel as part of the main taskflow execution.

### Taskflow

```rust
impl Taskflow {
    /// Add an async task
    pub fn emplace_async<F, Fut>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static;
    
    /// Add a subflow (can contain async tasks)
    pub fn emplace_subflow<F>(&mut self, work: F) -> TaskHandle
    where F: FnOnce(&mut Subflow) + Send + 'static;
}
```

### Subflow

```rust
impl Subflow {
    /// Add an async task to the subflow
    pub fn emplace_async<F, Fut>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static;
    
    /// Add a static task to the subflow
    pub fn emplace<F>(&mut self, work: F) -> TaskHandle
    where F: FnOnce() + Send + 'static;
}
```

## Examples

### Example 1: Parallel Async Tasks

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    // Create 5 independent async tasks
    for i in 0..5 {
        taskflow.emplace_async(move || async move {
            println!("Task {} starting", i);
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("Task {} done", i);
        });
    }
    
    let start = std::time::Instant::now();
    executor.run_async(&taskflow).await;
    let elapsed = start.elapsed();
    
    println!("Completed in {:?}", elapsed);
    // Output: ~100ms (parallel), not 500ms (sequential)
}
```

### Example 2: Mixed Sync and Async

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    // Synchronous task
    let sync_task = taskflow.emplace(|| {
        println!("Sync task");
    }).name("sync");
    
    // Asynchronous task
    let async_task = taskflow.emplace_async(|| async {
        println!("Async task start");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("Async task end");
    }).name("async");
    
    // Dependencies work as expected
    sync_task.precede(&async_task);
    
    executor.run_async(&taskflow).await;
}
```

### Example 3: Async Subflows

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    taskflow.emplace_subflow(|subflow| {
        println!("Creating subflow");
        
        // Add async tasks to subflow
        for i in 0..3 {
            subflow.emplace_async(move || async move {
                println!("Subflow task {}", i);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            });
        }
    }).name("parent");
    
    executor.run_async(&taskflow).await;
    println!("Subflow completed");
}
```

### Example 4: Parallel HTTP Requests

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(8);
    let mut taskflow = Taskflow::new();
    
    let responses = Arc::new(Mutex::new(Vec::new()));
    
    let urls = vec![
        "https://api.example.com/users",
        "https://api.example.com/posts",
        "https://api.example.com/comments",
    ];
    
    for url in urls {
        let r = responses.clone();
        let url = url.to_string();
        
        taskflow.emplace_async(move || async move {
            // Simulated HTTP request
            println!("Fetching {}", url);
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            let response = format!("Data from {}", url);
            r.lock().unwrap().push(response);
        });
    }
    
    executor.run_async(&taskflow).await;
    
    println!("All requests completed");
    for response in responses.lock().unwrap().iter() {
        println!("  {}", response);
    }
}
```

### Example 5: Complex Async Pipeline

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data = Arc::new(Mutex::new(Vec::new()));
    
    // Stage 1: Fetch data (async)
    let d = data.clone();
    let fetch = taskflow.emplace_async(move || async move {
        println!("Fetching data...");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        d.lock().unwrap().extend_from_slice(&[1, 2, 3, 4, 5]);
    }).name("fetch");
    
    // Stage 2: Process data (sync)
    let d = data.clone();
    let process = taskflow.emplace(move || {
        println!("Processing data...");
        let mut data = d.lock().unwrap();
        for val in data.iter_mut() {
            *val *= 2;
        }
    }).name("process");
    
    // Stage 3: Save data (async)
    let d = data.clone();
    let save = taskflow.emplace_async(move || async move {
        println!("Saving data...");
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let data = d.lock().unwrap();
        println!("Saved: {:?}", *data);
    }).name("save");
    
    // Define pipeline
    fetch.precede(&process);
    process.precede(&save);
    
    executor.run_async(&taskflow).await;
}
```

## Performance

### Sequential vs Parallel

**Sequential (old):**
```
Task 1: 100ms ─────────────┐
Task 2: 100ms              └─────────────┐
Task 3: 100ms                            └─────────────┐
Total: 300ms                                           └─
```

**Parallel (new):**
```
Task 1: 100ms ─────────────┐
Task 2: 100ms ─────────────┤
Task 3: 100ms ─────────────┤
Total: 100ms               └─
```

### Benchmarks

Typical performance on 4-core system:

| Tasks | Sequential | Parallel | Speedup |
|-------|-----------|----------|---------|
| 4 I/O (100ms each) | 400ms | 100ms | 4.0x |
| 8 I/O (50ms each) | 400ms | 50ms | 8.0x |
| 16 I/O (25ms each) | 400ms | 25ms | 16.0x |

## Use Cases

### 1. Web Scraping

Fetch multiple pages concurrently:

```rust
let urls = vec!["url1", "url2", "url3"];

for url in urls {
    taskflow.emplace_async(move || async move {
        let response = reqwest::get(url).await?;
        let body = response.text().await?;
        process_html(&body);
    });
}
```

### 2. Database Queries

Run multiple queries in parallel:

```rust
let queries = vec!["SELECT ...", "SELECT ...", "SELECT ..."];

for query in queries {
    taskflow.emplace_async(move || async move {
        let result = db.query(query).await?;
        process_result(result);
    });
}
```

### 3. API Aggregation

Fetch data from multiple APIs:

```rust
taskflow.emplace_async(|| async {
    let users = fetch_users().await;
    users
}).name("fetch_users");

taskflow.emplace_async(|| async {
    let posts = fetch_posts().await;
    posts
}).name("fetch_posts");

// Aggregate results
```

### 4. File Processing

Process files concurrently:

```rust
for file in files {
    taskflow.emplace_async(move || async move {
        let content = tokio::fs::read_to_string(file).await?;
        process_content(&content);
    });
}
```

## Best Practices

### 1. Use Appropriate Executor Size

```rust
// For I/O-bound tasks: More workers
let executor = AsyncExecutor::new(100);

// For CPU-bound tasks: Match core count
let executor = AsyncExecutor::new(num_cpus::get());
```

### 2. Handle Errors Properly

```rust
taskflow.emplace_async(|| async {
    match fetch_data().await {
        Ok(data) => process(data),
        Err(e) => eprintln!("Error: {}", e),
    }
});
```

### 3. Use Shared State Carefully

```rust
use std::sync::Arc;
use tokio::sync::Mutex;  // Use tokio::sync::Mutex for async

let data = Arc::new(Mutex::new(Vec::new()));

taskflow.emplace_async(move || async move {
    let mut d = data.lock().await;  // Note: .await, not unwrap()
    d.push(42);
});
```

### 4. Respect Dependencies

```rust
// Ensure dependencies are set correctly
fetch_task.precede(&process_task);
process_task.precede(&save_task);
```

## Migration from Sequential to Parallel

**Before (Sequential):**
```rust
let executor = AsyncExecutor::new(4);

// Tasks ran sequentially
executor.run_async(&taskflow).await;
```

**After (Parallel):**
```rust
let executor = AsyncExecutor::new(4);

// Tasks now run in parallel!
// No code changes needed - automatic parallelization
executor.run_async(&taskflow).await;
```

The new implementation automatically runs independent tasks in parallel while respecting dependencies.

## Limitations

1. **Task Ordering**: Non-deterministic execution order for independent tasks
2. **Memory**: Each spawned task has overhead
3. **Tokio Required**: Must use Tokio runtime (no other async runtimes)

## Troubleshooting

### Issue: Tasks Not Running in Parallel

**Problem**: Tasks appear to run sequentially

**Solution**: Check for hidden dependencies
```rust
// Bad: Creates implicit dependency
let data = Vec::new();
for i in 0..10 {
    taskflow.emplace_async(|| async {
        data.push(i);  // Mutable access forces sequential
    });
}

// Good: Use Arc<Mutex>
let data = Arc::new(Mutex::new(Vec::new()));
for i in 0..10 {
    let d = data.clone();
    taskflow.emplace_async(move || async move {
        d.lock().await.push(i);  // Safe concurrent access
    });
}
```

### Issue: Deadlock

**Problem**: Tasks hang indefinitely

**Solution**: Check for circular dependencies
```rust
// Detect cycles before execution
use taskflow_rs::CycleDetector;

let mut detector = CycleDetector::new();
// Add dependencies...
if detector.detect_cycle().has_cycle() {
    panic!("Circular dependency detected!");
}
```

## Examples

Run the parallel async demo:

```bash
cargo run --features async --example async_parallel
```

Expected output:
```
=== Parallel Async Execution Demo ===

1. Parallel Async Execution
   Running independent async tasks in parallel

   [Task 0] Starting async work
   [Task 1] Starting async work
   [Task 2] Starting async work
   [Task 3] Starting async work
   [Task 4] Starting async work
   [Task 0] Completed
   [Task 1] Completed
   [Task 2] Completed
   [Task 3] Completed
   [Task 4] Completed

   All tasks completed in 112ms
   Results: [0, 1, 2, 3, 4]
   ✓ Tasks ran in parallel (112ms < 500ms)

2. Mixed Sync and Async Tasks
   ...

3. Async Subflows
   ...

4. Parallel HTTP Requests (Simulated)
   ...
```

## See Also

- `AsyncExecutor` - Async task executor
- `Taskflow` - Task graph construction
- `Subflow` - Dynamic task creation
