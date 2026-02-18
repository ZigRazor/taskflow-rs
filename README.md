# TaskFlow-RS

A Rust implementation of [TaskFlow](https://taskflow.github.io/) - a general-purpose task-parallel programming library.

## Features

- ✅ **Task Graph Construction** - Build directed acyclic graphs (DAGs) of tasks with dependencies
- ✅ **Lock-Free Work-Stealing Executor** - High-performance multi-threaded scheduler with per-worker queues
- ✅ **Subflows** - Create nested task graphs for recursive parallelism
- ✅ **Condition Tasks** - Control flow with conditional branching
- ✅ **Graph Visualization** - Export task graphs to DOT format
- 🚧 **Parallel Algorithms** - for_each, reduce, sort (planned)
- 🚧 **Async Tasks** - Dynamic task creation (planned)
- 🚧 **GPU Support** - CUDA integration (planned)

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
