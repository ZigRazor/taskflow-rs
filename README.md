# TaskFlow-RS

A Rust implementation of [TaskFlow](https://taskflow.github.io/) — a general-purpose task-parallel programming library with heterogeneous CPU/GPU support.

## Features

- ✅ **Task Graph Construction** — Build directed acyclic graphs (DAGs) with flexible dependency management
- ✅ **Lock-Free Work-Stealing Executor** — High-performance multi-threaded scheduler with per-worker queues
- ✅ **Subflows** — Nested task graphs for recursive parallelism
- ✅ **Condition Tasks** — Conditional branching and loop constructs
- ✅ **Cycle Detection** — Prevent cycles at graph construction time
- ✅ **Parallel Algorithms** — `for_each`, `reduce`, `transform`, `sort`, `scan`
- ✅ **Async Task Support** — Full `async`/`await` integration with Tokio
- ✅ **Pipeline Support** — Stream processing with parallel/serial stages and backpressure
- ✅ **Composition** — Reusable, parameterized task graph components
- ✅ **Run Variants** — Run taskflows N times, until a condition, or concurrently
- ✅ **GPU Support** — CUDA, OpenCL, and ROCm/HIP backends with async transfers and multiple streams
- ✅ **Advanced Features** — Task priorities, cancellation, custom schedulers, NUMA-aware scheduling
- ✅ **Tooling** — Profiler, DOT/SVG/HTML visualization, performance monitoring, debug logging
- ✅ **Type-Safe Pipelines** — Compile-time type checking across pipeline stages
- ✅ **Built-in Metrics** — Comprehensive execution metrics and monitoring

## Quick Start

```toml
[dependencies]
taskflow-rs = "0.2"
```

### Basic Example

```rust
use taskflow_rs::{Executor, Taskflow};

fn main() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let a = taskflow.emplace(|| println!("Task A")).name("A");
    let b = taskflow.emplace(|| println!("Task B")).name("B");
    let c = taskflow.emplace(|| println!("Task C")).name("C");
    let d = taskflow.emplace(|| println!("Task D")).name("D");

    a.precede(&b);  // A → B
    a.precede(&c);  // A → C
    d.succeed(&b);  // B → D
    d.succeed(&c);  // C → D

    executor.run(&taskflow).wait();
    // Graph: A → B → D
    //          ↘ C ↗
}
```

---

## GPU Support

TaskFlow-RS provides a **backend-agnostic GPU API** supporting CUDA (NVIDIA), OpenCL (NVIDIA/AMD/Intel), and ROCm/HIP (AMD). All three share the same user-facing types — switching hardware requires changing one line.

### Building with GPU Support

```bash
# CUDA (NVIDIA) — default CUDA 12.0
cargo build --features gpu

# OpenCL (NVIDIA / AMD / Intel)
# Ubuntu: sudo apt install ocl-icd-opencl-dev
cargo build --features opencl

# ROCm / HIP (AMD) — requires ROCm ≥ 5.0
ROCM_PATH=/opt/rocm cargo build --features rocm

# All backends
cargo build --features all-gpu

# No GPU (stub backend — always available, no flags needed)
cargo build
```

### Backend Selection

```rust
use taskflow_rs::{GpuDevice, BackendKind};

// Auto-select: CUDA → ROCm → OpenCL → Stub
let device = GpuDevice::new(0)?;

// Explicit backend
let device = GpuDevice::with_backend(0, BackendKind::OpenCL)?;
let device = GpuDevice::with_backend(0, BackendKind::Rocm)?;

println!("{} ({:?})", device.name(), device.backend_kind());
```

### Synchronous Transfers

```rust
use taskflow_rs::{GpuDevice, GpuBuffer};

let device = GpuDevice::new(0)?;
let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&device, 1024)?;

let src: Vec<f32> = (0..1024).map(|i| i as f32).collect();
buf.copy_from_host(&src)?;          // blocking H2D

let mut dst = vec![0.0f32; 1024];
buf.copy_to_host(&mut dst)?;        // blocking D2H
```

### Asynchronous Transfers & Multiple Streams

```rust
use taskflow_rs::{GpuDevice, GpuBuffer, GpuStream};

let device = GpuDevice::new(0)?;

// ── Stream pool — 4 streams, round-robin assignment ──────────────────────
let pool = device.stream_pool(4)?;

for batch in batches {
    let guard  = pool.acquire()?;
    let stream = guard.stream();

    let mut buf = GpuBuffer::allocate(&device, batch.len())?;

    // Enqueue async H2D — returns immediately on CPU
    unsafe { buf.copy_from_host_async(&batch, stream)?; }
}
pool.synchronize_all()?;            // wait for all streams

// ── Safe async transfers with Tokio ─────────────────────────────────────
let (buf, data) = buf.copy_from_host_async_owned(data).await?;
let (buf, result) = buf.copy_to_host_async_owned(result).await?;

// ── Double-buffer pipeline (StreamSet) ──────────────────────────────────
let set = device.stream_set(2, "pipeline")?;

for (i, batch) in batches.enumerate() {
    if i >= 2 { set.get(i).synchronize()?; }  // sync slot before reuse

    let stream = set.get(i);
    unsafe {
        dev_bufs[i % 2].copy_from_host_async(batch, stream)?;
        dev_bufs[i % 2].copy_to_host_async(&mut results[i % 2], stream)?;
    }
}
set.synchronize_all()?;
```

### Multi-Stream in a TaskFlow DAG

```rust
use std::sync::Arc;
use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer};

let device = Arc::new(GpuDevice::new(0)?);
let pool   = Arc::new(device.stream_pool(4)?);

let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

// Each GPU task picks a stream from the shared pool
let tasks: Vec<_> = (0..4).map(|i| {
    let pool_ref   = Arc::clone(&pool);
    let device_ref = Arc::clone(&device);

    taskflow.emplace(move || {
        let guard  = pool_ref.acquire().unwrap();
        let stream = guard.stream();

        let mut buf = GpuBuffer::<f32>::allocate(&device_ref, 1024).unwrap();
        unsafe { buf.copy_from_host_async(&input[i], stream).unwrap(); }
        stream.synchronize().unwrap();
    })
}).collect();

executor.run(&taskflow).wait();
pool.synchronize_all()?;
```

### Heterogeneous CPU+GPU Pipeline

```rust
use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer};
use std::sync::{Arc, Mutex};

let device = GpuDevice::new(0).expect("GPU required");
let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

let data = Arc::new(Mutex::new(Vec::<f32>::new()));

// Stage 1 — CPU: generate data
let d1 = data.clone();
let generate = taskflow.emplace(move || {
    *d1.lock().unwrap() = (0..1024).map(|i| i as f32).collect();
});

// Stage 2 — GPU: process on device
let d2 = data.clone();
let dev = device.clone();
let process = taskflow.emplace(move || {
    let data = d2.lock().unwrap();
    let mut buf = GpuBuffer::allocate(&dev, data.len()).unwrap();
    buf.copy_from_host(&data).unwrap();
    dev.synchronize().unwrap();
});

// Stage 3 — CPU: validate
let validate = taskflow.emplace(|| println!("Validated ✓"));

generate.precede(&process);
process.precede(&validate);
executor.run(&taskflow).wait();
```

**See [GPU.md](GPU.md) for the full GPU API reference.**

---

## Async Tasks

```rust
use taskflow_rs::AsyncExecutor;

let executor = AsyncExecutor::new(4);
let mut taskflow = Taskflow::new();

taskflow.emplace_async(|| async {
    let result = fetch_data().await;
    process(result).await;
});

executor.run_async(&taskflow).await;

// Async run variants
executor.run_n_async(10, || create_taskflow()).await;
executor.run_until_async(|| create_taskflow(), || done()).await;
```

**Build with:** `cargo build --features async`  
**See [ASYNC_TASKS.md](ASYNC_TASKS.md) for full documentation.**

---

## Advanced Features

```rust
use taskflow_rs::{
    Priority, CancellationToken,
    PriorityScheduler, Scheduler, NumaTopology,
};

// Task priorities
let mut scheduler = PriorityScheduler::new();
scheduler.push(1, Priority::Critical);
scheduler.push(2, Priority::Normal);
scheduler.push(3, Priority::Low);

// Cooperative cancellation
let token = CancellationToken::new();
let t = token.clone();
taskflow.emplace(move || {
    for i in 0..100 {
        if t.is_cancelled() { return; }
        process(i);
    }
});
token.cancel();

// NUMA awareness
let topology = NumaTopology::detect();
for node in &topology.nodes {
    println!("NUMA node {}: {} CPUs", node.id, node.cpus.len());
}
```

**See [ADVANCED_FEATURES.md](ADVANCED_FEATURES.md) for full documentation.**

---

## Run Variants

```rust
// Run N taskflows in parallel
executor.run_n(10, || create_taskflow()).wait();

// Run until a condition is met
executor.run_until(|| create_taskflow(), || counter.load(Relaxed) >= 50).wait();

// Run multiple flows concurrently
executor.run_many_and_wait(&[&flow1, &flow2, &flow3]);
```

**See [PARALLEL_RUN.md](PARALLEL_RUN.md) and [RUN_VARIANTS.md](RUN_VARIANTS.md).**

---

## Tooling

```rust
use taskflow_rs::{Profiler, generate_dot_graph, PerformanceMetrics};

// Profiling
let profiler = Profiler::new(4);
profiler.enable();
executor.run(&taskflow).wait();
let profile = profiler.get_profile().unwrap();
println!("Total: {:?}", profile.total_duration);

// Visualization — export to DOT / SVG / HTML
generate_dot_graph(&taskflow, "graph.dot");

// Real-time metrics
let metrics = PerformanceMetrics::new();
println!("Tasks/sec: {:.1}", metrics.tasks_per_second());
```

**See [TOOLING.md](TOOLING.md) for full documentation.**

---

## Architecture

### Work-Stealing Executor

```
Worker 0: [Task] [Task] [Task]  ← Push/Pop (LIFO, own queue)
                ↓ steal (FIFO)
Worker 1: [Task] [Task]         ← Idle workers steal from busy ones
```

- Per-worker lock-free deques (`crossbeam::deque`)
- LIFO for own tasks (cache-warm), FIFO for stolen tasks
- Near-linear scalability on multi-core systems

### GPU Backend Architecture

```
GpuDevice
  └── Arc<dyn ComputeBackend>   ← runtime dispatch
        ├── CudaBackend         (--features gpu)
        ├── OpenCLBackend       (--features opencl)
        ├── RocmBackend         (--features rocm)
        └── StubBackend         (always available)

GpuBuffer<T>  ──► DeviceBuffer (Arc<dyn BackendBuffer>)
GpuStream     ──► DeviceStream (Arc<dyn BackendStream>)
StreamPool    ──► N × GpuStream  (round-robin or least-pending)
StreamSet     ──► fixed-depth stream array for pipelining
```

### Comparison with C++ TaskFlow

| Feature | C++ TaskFlow | TaskFlow-RS |
|---------|:-----------:|:-----------:|
| Task Graphs | ✅ | ✅ |
| Work Stealing | ✅ | ✅ |
| Subflows | ✅ | ✅ |
| Condition Tasks | ✅ | ✅ |
| Parallel Algorithms | ✅ | ✅ |
| Async Tasks | ✅ | ✅ |
| Pipeline | ✅ | ✅ |
| GPU — CUDA | ✅ | ✅ |
| GPU — OpenCL | ✅ | ✅ |
| GPU — ROCm/HIP | ✅ | ✅ |
| Async GPU Transfers | ✅ | ✅ |
| Multiple GPU Streams | ✅ | ✅ |
| NUMA Awareness | ✅ | ✅ |

---

## Building

```bash
# Core (no GPU)
cargo build

# With CUDA
cargo build --features gpu

# With OpenCL
cargo build --features opencl

# With ROCm
ROCM_PATH=/opt/rocm cargo build --features rocm

# All GPU backends + async
cargo build --features all-gpu,async

# Release build
cargo build --release --features gpu

# Run tests
cargo test
cargo test --features gpu      # GPU tests (requires hardware, mostly #[ignore])
```

## Examples

```bash
# GPU async streams + multi-stream demo (stub — no GPU needed)
cargo run --example gpu_async_streams

# With CUDA
cargo run --features gpu --example gpu_async_streams
cargo run --features gpu --example gpu_tasks
cargo run --features gpu --example gpu_pipeline

# Async tasks
cargo run --features async --example async_tasks
cargo run --features async --example async_parallel
```

## Documentation

| Document | Contents |
|----------|----------|
| [GPU.md](GPU.md) | Full GPU API: backends, streams, async transfers, patterns |
| [GPU_SETUP.md](GPU_SETUP.md) | CUDA version configuration, ROCm install, troubleshooting |
| [DESIGN.md](DESIGN.md) | Architecture decisions, implementation status, feature checklist |
| [ASYNC_TASKS.md](ASYNC_TASKS.md) | Async executor and task documentation |
| [ADVANCED_FEATURES.md](ADVANCED_FEATURES.md) | Priorities, cancellation, schedulers, NUMA |
| [PIPELINE.md](PIPELINE.md) | Concurrent pipeline documentation |
| [TOOLING.md](TOOLING.md) | Profiler, visualization, monitoring |

## License

MIT
