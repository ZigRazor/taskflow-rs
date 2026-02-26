# TaskFlow-RS

A Rust implementation of [TaskFlow](https://taskflow.github.io/) - a general-purpose task-parallel programming library.

## Features

- ✅ **Task Graph Construction** - Build directed acyclic graphs (DAGs) of tasks with dependencies
- ✅ **Lock-Free Work-Stealing Executor** - High-performance multi-threaded scheduler with per-worker queues
- ✅ **Subflows** - Create nested task graphs for recursive parallelism
- ✅ **Condition Tasks** - Control flow with conditional branching
- ✅ **Parallel Algorithms** - `for_each`, `reduce`, `transform`, `sort` primitives
- ✅ **Async Task Support** - Integration with Rust's async/await and Tokio runtime
- ✅ **Pipeline Support** - Stream processing with parallel/serial stages, token management, and backpressure
- ✅ **Composition** - Build complex workflows from reusable task graph components
- ✅ **Run Variants** - Execute taskflows N times, until conditions, or concurrently
- ✅ **GPU Support** - CUDA integration for heterogeneous CPU-GPU computing
- ✅ **Graph Visualization** - Export task graphs to DOT format

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
taskflow-rs = "0.1"
```

### Basic Example

```rust
use taskflow_rs::{Executor, Taskflow};

fn main() {
    let mut executor = Executor::new(4); // 4 worker threads
    let mut taskflow = Taskflow::new();

    // Create tasks
    let a = taskflow.emplace(|| {
        println!("Task A");
    }).name("A");

    let b = taskflow.emplace(|| {
        println!("Task B");
    }).name("B");

    let c = taskflow.emplace(|| {
        println!("Task C");
    }).name("C");

    let d = taskflow.emplace(|| {
        println!("Task D");
    }).name("D");

    // Define dependencies
    a.precede(&b);  // A runs before B
    a.precede(&c);  // A runs before C
    d.succeed(&b);  // D runs after B
    d.succeed(&c);  // D runs after C

    // Execute
    executor.run(&taskflow).wait();
}
```

This creates the following task graph:

```
A → B → D
  ↘ C ↗
```

### Subflow Example

Create nested task graphs for recursive parallelism:

```rust
let mut taskflow = Taskflow::new();

let parent = taskflow.emplace_subflow(|subflow| {
    let child1 = subflow.emplace(|| {
        println!("Child task 1");
    }).name("child1");

    let child2 = subflow.emplace(|| {
        println!("Child task 2");
    }).name("child2");

    child1.precede(&child2);
}).name("parent");

executor.run(&taskflow).wait();
```

### Graph Visualization

Export your task graph to DOT format for visualization:

```rust
let dot = taskflow.dump();
println!("{}", dot);
```

Paste the output at [GraphViz Online](https://dreampuf.github.io/GraphvizOnline/) to visualize.

### Parallel Algorithms

TaskFlow-RS provides high-level parallel algorithm primitives:

#### Parallel For-Each

Apply a function to each element in parallel:

```rust
use taskflow_rs::{Executor, Taskflow, parallel_for_each};

let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

let data: Vec<i32> = (0..1000).collect();

parallel_for_each(&mut taskflow, data, 250, |item| {
    // Process each item in parallel
    println!("Processing: {}", item);
});

executor.run(&taskflow).wait();
```

#### Parallel Reduce

Reduce a collection to a single value in parallel:

```rust
use taskflow_rs::parallel_reduce;

let data: Vec<i32> = (1..=1000).collect();

let (_task, result) = parallel_reduce(
    &mut taskflow,
    data,
    250,
    0,  // identity value
    |acc, item| acc + item
);

executor.run(&taskflow).wait();

// Get the result
let sum = *result.lock().unwrap();
println!("Sum: {}", sum);  // 500500
```

#### Parallel Transform

Map elements in parallel and collect results:

```rust
use taskflow_rs::parallel_transform;

let data: Vec<i32> = (0..1000).collect();

let (_tasks, results) = parallel_transform(
    &mut taskflow,
    data,
    250,
    |x| x * x  // Square each element
);

executor.run(&taskflow).wait();

let results = results.lock().unwrap();
println!("Transformed {} items", results.len());
```

#### Parallel Sort

Sort elements in parallel using merge sort:

```rust
use taskflow_rs::parallel_sort;

let data: Vec<i32> = vec![5, 2, 8, 1, 9, 3];

parallel_sort(&mut taskflow, data, 100, |a, b| a.cmp(b));

executor.run(&taskflow).wait();
```

### Async Tasks

TaskFlow-RS supports asynchronous tasks with Tokio integration (requires `async` feature):

```toml
[dependencies]
taskflow-rs = { version = "0.1", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

#### Basic Async Tasks

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let task = taskflow.emplace_async(|| async {
        // Async work
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("Done!");
    });
    
    executor.run(&taskflow);
}
```

#### Mixed Sync and Async

```rust
// Sync task
let sync = taskflow.emplace(|| {
    println!("Sync work");
});

// Async task
let async_task = taskflow.emplace_async(|| async {
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Async work");
});

// Dependencies work across sync/async
sync.precede(&async_task);
```

**See [ASYNC_TASKS.md](ASYNC_TASKS.md) for comprehensive async documentation.**

### Pipeline Processing

TaskFlow-RS provides concurrent pipelines for stream processing with automatic backpressure:

```rust
use taskflow_rs::pipeline::{ConcurrentPipeline, Token};
use std::thread;

let pipeline = ConcurrentPipeline::new(
    10,   // buffer_size (backpressure threshold)
    100   // max_tokens (total capacity)
);

// Producer thread
let p = pipeline.clone();
thread::spawn(move || {
    for i in 0..100 {
        p.push(i).unwrap();
    }
    p.stop();
});

// Consumer thread
let c = pipeline.clone();
thread::spawn(move || {
    while !c.is_stopped() {
        if let Some(token) = c.try_pop() {
            println!("Processed: {}", token.data);
        }
    }
});
```

#### Multi-Stage Pipeline

```rust
use taskflow_rs::pipeline::ConcurrentPipeline;

// Create pipeline stages
let input = ConcurrentPipeline::new(10, 100);
let output = ConcurrentPipeline::new(10, 100);

// Parallel processing stage (4 workers)
for worker_id in 0..4 {
    let i = input.clone();
    let o = output.clone();
    
    thread::spawn(move || {
        while let Some(token) = i.try_pop() {
            let result = process(token.data);
            o.push(result).ok();
        }
    });
}
```

**Features:**
- Token management with unique IDs
- Automatic backpressure handling
- Thread-safe concurrent access
- Configurable buffer sizes

**See [PIPELINE.md](PIPELINE.md) for comprehensive pipeline documentation.**

### Composition

Build complex workflows from reusable task graph components:

```rust
use taskflow_rs::{Taskflow, Composition, CompositionBuilder, TaskflowComposable};

// Create a reusable component
fn create_processor() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let load = builder.taskflow_mut().emplace(|| {
        println!("Load");
    });
    
    let process = builder.taskflow_mut().emplace(|| {
        println!("Process");
    });
    
    let save = builder.taskflow_mut().emplace(|| {
        println!("Save");
    });
    
    load.precede(&process);
    process.precede(&save);
    
    // Define entry/exit points
    builder.mark_entries(&[load]);
    builder.mark_exits(&[save]);
    
    builder.build()
}

// Use the component multiple times
let mut main_flow = Taskflow::new();
let processor = create_processor();

// Compose it three times in parallel
let start = main_flow.emplace(|| println!("Start"));
let end = main_flow.emplace(|| println!("End"));

for _ in 0..3 {
    let instance = main_flow.compose(&processor);
    
    // Connect to workflow
    for entry in instance.entries() {
        start.precede(entry);
    }
    for exit in instance.exits() {
        exit.precede(&end);
    }
}
```

**Features:**
- Reusable task graph components
- Multiple entry/exit points
- Sequential and parallel composition
- Fan-out/fan-in patterns

**See [COMPOSITION.md](COMPOSITION.md) for comprehensive composition documentation.**

### Run Variants

Execute taskflows with flexible patterns - multiple times, conditionally, or concurrently:

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let mut executor = Executor::new(4);
let counter = Arc::new(AtomicUsize::new(0));

// Run N times (factory pattern)
let c = counter.clone();
executor.run_n(10, move || {
    let mut taskflow = Taskflow::new();
    let c = c.clone();
    taskflow.emplace(move || {
        c.fetch_add(1, Ordering::Relaxed);
        println!("Processing...");
    });
    taskflow
}).wait();

// Run until condition
let c = counter.clone();
let c_check = counter.clone();
executor.run_until(
    move || {
        let mut taskflow = Taskflow::new();
        let c = c.clone();
        taskflow.emplace(move || {
            c.fetch_add(1, Ordering::Relaxed);
        });
        taskflow
    },
    move || c_check.load(Ordering::Relaxed) >= 50
).wait();

// Run multiple flows concurrently
let flow1 = create_flow_1();
let flow2 = create_flow_2();
let flow3 = create_flow_3();

executor.run_many_and_wait(&[&flow1, &flow2, &flow3]);
```

**Use cases:**
- Batch processing (run_n)
- Convergence loops (run_until)
- Parallel pipelines (run_many)
- Retry logic
- Training epochs

**See [RUN_VARIANTS.md](RUN_VARIANTS.md) for comprehensive documentation.**

### GPU Support

Integrate CUDA GPU tasks seamlessly into your task graphs for heterogeneous computing:

```rust
use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer};
use std::sync::Arc;

// Initialize GPU
let device = GpuDevice::new(0).expect("CUDA device required");

let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

let data = Arc::new(std::sync::Mutex::new(Vec::new()));

// CPU task: Generate data
let d1 = data.clone();
let generate = taskflow.emplace(move || {
    let mut data = d1.lock().unwrap();
    *data = (0..1024).map(|i| i as f32).collect();
    println!("Generated data on CPU");
});

// GPU task: Process on device
let d2 = data.clone();
let dev = device.clone();
let process_gpu = taskflow.emplace(move || {
    let data = d2.lock().unwrap();
    
    // Allocate GPU memory
    let mut gpu_buf = GpuBuffer::allocate(&dev, data.len()).unwrap();
    
    // Transfer to GPU
    gpu_buf.copy_from_host(&data).unwrap();
    
    // Compute on GPU
    dev.synchronize().unwrap();
    
    println!("Processed on GPU");
});

// CPU task: Validate
let validate = taskflow.emplace(|| {
    println!("Validated results");
});

// Build heterogeneous pipeline
generate.precede(&process_gpu);
process_gpu.precede(&validate);

executor.run(&taskflow).wait();
```

**Features:**
- CUDA integration via cudarc
- Efficient host-device data transfers
- GPU-CPU synchronization
- Multi-GPU support
- Heterogeneous task graphs

**Prerequisites:**
- NVIDIA GPU with CUDA support
- CUDA Toolkit 11.0+
- Default uses CUDA 12.0 - if you have a different version, edit the `gpu` feature in `Cargo.toml` (see GPU_SETUP.md)
- Build with `--features gpu`

**See [GPU.md](GPU.md) for comprehensive GPU documentation.**

## API Overview

### Taskflow

The main container for building task graphs.

```rust
let mut taskflow = Taskflow::new();

// Create a static task
let task = taskflow.emplace(|| {
    // Task work
}).name("task_name");

// Create a subflow task
let subflow_task = taskflow.emplace_subflow(|subflow| {
    // Create child tasks
}).name("subflow_name");

// Create a condition task
let cond = taskflow.emplace_condition(|| {
    // Return index of successor to execute
    0
}).name("condition");

// Get task count
let count = taskflow.size();

// Export to DOT
let dot = taskflow.dump();
```

### TaskHandle

Handle to a task for building dependencies.

```rust
// Define dependencies
task_a.precede(&task_b);  // A → B
task_c.succeed(&task_b);  // B → C

// Set task name
let task = taskflow.emplace(|| {}).name("MyTask");
```

### Executor

Thread pool for executing taskflows.

```rust
// Create executor with N workers (0 = auto-detect)
let mut executor = Executor::new(4);

// Run taskflow once
let future = executor.run(&taskflow);
future.wait();

// Block until completion
executor.wait_for_all();
```

## Architecture

### Work-Stealing Executor

TaskFlow-RS uses a high-performance **work-stealing scheduler** for task execution:

- **Per-worker queues**: Each thread has its own lock-free deque
- **Lock-free operations**: Push/pop from own queue without locks
- **Work stealing**: Idle workers steal tasks from busy workers
- **Cache locality**: LIFO execution for own tasks, FIFO for stolen tasks
- **Excellent scalability**: Near-linear speedup on multi-core systems

```
Worker 0: [Task] [Task] [Task]  ← Push/Pop (LIFO)
           ↓ Steal (FIFO)
Worker 1: [Task] [Task]         ← Idle workers steal from busy ones
```

See [WORK_STEALING.md](WORK_STEALING.md) for detailed implementation notes.

### Task Representation

Each task is represented as a node in a directed acyclic graph (DAG):

- **Work**: Closure to execute (static, subflow, or condition)
- **Dependencies**: Set of tasks that must complete before this task
- **Successors**: Set of tasks to execute after this task

### Executor Design

The executor uses a multi-threaded work-stealing scheduler:

1. **Initialization**: Tasks with no dependencies are queued
2. **Execution**: Workers pull tasks from the queue and execute them
3. **Dependency Resolution**: When a task completes, successor dependencies are decremented
4. **Ready Queue**: Tasks with satisfied dependencies are enqueued
5. **Completion**: All workers exit when no tasks remain

### Thread Safety

- Task graphs use `Arc<Mutex<>>` for shared ownership
- Worker synchronization via `Condvar`
- Lock-free queuing where possible

## Comparison with C++ TaskFlow

| Feature | C++ TaskFlow | TaskFlow-RS | Status |
|---------|-------------|-------------|--------|
| Task Graphs | ✅ | ✅ | Complete |
| Subflows | ✅ | ✅ | Complete |
| Conditional Tasks | ✅ | ✅ | Basic |
| Parallel Algorithms | ✅ | 🚧 | Planned |
| Async Tasks | ✅ | 🚧 | Planned |
| GPU Support | ✅ | 🚧 | Planned |
| Pipeline | ✅ | 🚧 | Planned |

## Roadmap

- [ ] Parallel algorithm primitives (for_each, reduce, transform, sort)
- [ ] Async task support (dynamic task creation)
- [ ] Pipeline support for stream processing
- [ ] Better condition task handling (multi-way branching)
- [ ] GPU task support (CUDA/ROCm)
- [ ] Task profiling and visualization tools
- [ ] Composition support (combining taskflows)
- [ ] Run multiple times / until condition

## Examples

Run the examples:

```bash
cargo run --example basic
cargo run --example parallel_patterns
cargo run --example benchmark --release  # Run with optimizations for accurate results
```

### Running Benchmarks

To see the work-stealing performance benefits:

```bash
# Build with optimizations
cargo build --release --example benchmark

# Run benchmark
cargo run --release --example benchmark
```

Expected output shows near-linear scaling:
```
Wide Graph (1000 independent tasks):
  1 worker:  ~850ms
  2 workers: ~430ms (1.97x speedup)
  4 workers: ~220ms (3.86x speedup)
  8 workers: ~115ms (7.39x speedup)
```

## Contributing

Contributions welcome! Areas for improvement:

- Performance optimization (lock-free queues, better work-stealing)
- More comprehensive testing
- Parallel algorithm implementations
- Documentation and examples
- Benchmarking against C++ TaskFlow

## License

MIT License - see LICENSE file for details

## Acknowledgments

Inspired by [TaskFlow](https://taskflow.github.io/) by Dr. Tsung-Wei Huang and contributors.

## References

- [TaskFlow GitHub](https://github.com/taskflow/taskflow)
- [TaskFlow Paper](https://tsung-wei-huang.github.io/papers/tpds21-taskflow.pdf)
- [Work-Stealing Algorithms](https://en.wikipedia.org/wiki/Work_stealing)
