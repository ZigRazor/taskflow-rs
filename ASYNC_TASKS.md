# Async Task Support

TaskFlow-RS provides comprehensive support for asynchronous tasks, allowing you to seamlessly integrate with Rust's async/await ecosystem.

## Features

- **Async Tasks**: Create tasks that return `Future`s
- **Mixed Workflows**: Combine synchronous and asynchronous tasks
- **Tokio Integration**: Built on the Tokio async runtime
- **Parallel Async Execution**: Run multiple async tasks concurrently
- **Dependency Management**: Async tasks can depend on sync tasks and vice versa

## Enabling Async Support

Add the `async` feature to your `Cargo.toml`:

```toml
[dependencies]
taskflow-rs = { version = "0.1", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

## Basic Usage

### Creating Async Tasks

**When already in an async context** (e.g., inside `#[tokio::main]`):

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    // Use new() to reuse the current runtime
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let task = taskflow.emplace_async(|| async {
        // Async work here
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("Task completed!");
    });
    
    // Use run_async() when already in async context
    executor.run_async(&taskflow).await;
}
```

**When NOT in an async context** (standalone/sync code):

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

fn main() {
    // Use new_with_runtime() to create its own runtime
    let executor = AsyncExecutor::new_with_runtime(4);
    let mut taskflow = Taskflow::new();
    
    let task = taskflow.emplace_async(|| async {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("Task completed!");
    });
    
    // Use run() to block until completion
    executor.run(&taskflow);
}
```

**Important:** 
- Use `new()` + `run_async().await` when you're already in an async context
- Use `new_with_runtime()` + `run()` when starting from sync code

### Mixing Sync and Async Tasks

```rust
#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    // Synchronous task
    let sync_task = taskflow.emplace(|| {
        println!("Sync work");
    });

    // Asynchronous task
    let async_task = taskflow.emplace_async(|| async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("Async work");
    });

    // Async task depends on sync task
    sync_task.precede(&async_task);

    executor.run_async(&taskflow).await;
}
```

## Common Patterns

### Parallel Async I/O

```rust
#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    // Spawn multiple async HTTP requests
    for i in 0..10 {
        taskflow.emplace_async(move || async move {
            let response = fetch_data(i).await;
            process(response);
        });
    }

    executor.run_async(&taskflow).await;
}
```

### Async Pipeline

```rust
let data = Arc::new(Mutex::new(Vec::new()));

// Fetch
let fetch = taskflow.emplace_async({
    let data = Arc::clone(&data);
    move || async move {
        let items = fetch_from_api().await;
        *data.lock().unwrap() = items;
    }
});

// Transform
let transform = taskflow.emplace_async({
    let data = Arc::clone(&data);
    move || async move {
        let mut d = data.lock().unwrap();
        for item in d.iter_mut() {
            *item = transform_async(item).await;
        }
    }
});

// Save
let save = taskflow.emplace_async({
    let data = Arc::clone(&data);
    move || async move {
        let d = data.lock().unwrap();
        save_to_db(&*d).await;
    }
});

fetch.precede(&transform);
transform.precede(&save);
```

### Async with Conditionals

```rust
let mut condition = taskflow.emplace_conditional(|| {
    // Determine which branch based on some logic
    if check_cache() { 0 } else { 1 }
});

// Branch 0: Load from cache (fast)
let load_cache = taskflow.emplace_async(|| async {
    load_from_cache().await
});

// Branch 1: Fetch from network (slow)
let fetch_network = taskflow.emplace_async(|| async {
    fetch_from_network().await
});

condition.branch(0, &load_cache);
condition.branch(1, &fetch_network);

taskflow.register_branches(&condition);
```

## API Reference

### `AsyncExecutor`

```rust
pub struct AsyncExecutor { /* ... */ }

impl AsyncExecutor {
    /// Create a new async executor that uses the current Tokio runtime
    /// Use this when you're already in an async context (e.g., #[tokio::main])
    /// Panics if called outside of a Tokio runtime context
    pub fn new(num_workers: usize) -> Self;
    
    /// Create a new async executor with its own runtime
    /// Use this when you're NOT in an async context
    pub fn new_with_runtime(num_workers: usize) -> Self;
    
    /// Run a taskflow asynchronously
    /// Use this when you're already in an async context
    pub async fn run_async(&self, taskflow: &Taskflow);
    
    /// Run a taskflow and block until completion
    /// Use this when you need to block from sync code
    pub fn run(&self, taskflow: &Taskflow);
}
```

### `Taskflow::emplace_async`

```rust
pub fn emplace_async<F, Fut>(&mut self, work: F) -> TaskHandle
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static
```

Creates an asynchronous task that returns a `Future`.

## Performance Considerations

1. **Runtime Overhead**: Async tasks have slightly more overhead than sync tasks due to the async runtime
2. **CPU-Bound Work**: For CPU-intensive tasks, prefer sync tasks
3. **I/O-Bound Work**: For I/O operations (network, disk), async tasks excel
4. **Mixed Workloads**: Use async for I/O, sync for CPU work in the same workflow

## Examples

See `examples/async_tasks.rs` for comprehensive examples:

```bash
cargo run --features async --example async_tasks
```

## Limitations

1. **Feature Flag Required**: Async support requires the `async` feature
2. **Tokio Dependency**: Currently only supports Tokio runtime
3. **No Async Subflows**: Subflows don't yet support async tasks (planned)
4. **Sequential Execution**: Current async executor processes tasks sequentially (parallel execution planned)

## Future Enhancements

- Parallel async task execution
- Async subflows
- Support for other async runtimes (async-std)
- Async condition tasks
- Stream-based async pipelines
