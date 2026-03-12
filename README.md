# TaskFlow-RS

<p align="center">
  <b>A high-performance task graph runtime for Rust</b>
  <br>
  Build and execute dependency-based task graphs with parallel scheduling.
</p>

<p align="center">
  <a href="https://crates.io/crates/taskflowrs">
    <img src="https://img.shields.io/crates/v/taskflowrs.svg">
  </a>
  <a href="https://docs.rs/taskflowrs">
    <img src="https://docs.rs/taskflowrs/badge.svg">
  </a>
  <a href="https://github.com/ZigRazor/taskflow-rs/actions">
    <img src="https://github.com/ZigRazor/taskflow-rs/actions/workflows/ci.yml/badge.svg">
  </a>
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg">
  </a>
</p>

---

A Rust implementation of [TaskFlow](https://taskflow.github.io/) — a general-purpose
task-parallel programming library with heterogeneous CPU/GPU support.

## Features

- ✅ **Task Graph Construction** — DAGs with flexible dependency management
- ✅ **Lock-Free Work-Stealing Executor** — Per-worker queues, near-linear scalability
- ✅ **Subflows** — Nested task graphs for recursive parallelism
- ✅ **Condition Tasks** — Conditional branching and loop constructs
- ✅ **Cycle Detection** — Catches cycles at graph construction time
- ✅ **Parallel Algorithms** — `for_each`, `reduce`, `transform`, `sort`, `scan`
- ✅ **Async Task Support** — Full `async`/`await` integration with Tokio
- ✅ **Pipeline Support** — Stream processing with parallel/serial stages and backpressure
- ✅ **Composition** — Reusable, parameterized task graph components
- ✅ **GPU Support** — CUDA, OpenCL, and ROCm/HIP with async transfers and multiple streams
- ✅ **Task Priorities** — Static `Low / Normal / High / Critical` priority levels
- ✅ **Preemptive Cancellation** — Watchdog timeouts, RAII deadline guards, signal preemption
- ✅ **Dynamic Priority Adjustment** — O(log n) live reprioritization of queued tasks
- ✅ **Hardware Topology Integration** — hwloc2 / sysfs cache hierarchy, NUMA binding, CPU affinity
- ✅ **Tooling** — Profiler, DOT/SVG/HTML visualization, performance monitoring, debug logging

---

## Quick Start

```toml
[dependencies]
taskflow-rs = "0.2"
```

### Basic task graph

```rust
use taskflow_rs::{Executor, Taskflow};

fn main() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let a = taskflow.emplace(|| println!("A")).name("A");
    let b = taskflow.emplace(|| println!("B")).name("B");
    let c = taskflow.emplace(|| println!("C")).name("C");
    let d = taskflow.emplace(|| println!("D")).name("D");

    a.precede(&b);  // A → B
    a.precede(&c);  // A → C
    d.succeed(&b);  // B → D
    d.succeed(&c);  // C → D

    executor.run(&taskflow).wait();
    // Execution: A → {B, C} → D
}
```

---

## Preemptive Cancellation

TaskFlow-RS provides three escalating levels of cancellation, all sharing the same
`PreemptiveCancellationToken` type.

### Level 1 — Cooperative (polling)

```rust
use taskflow_rs::PreemptiveCancellationToken;

let token = PreemptiveCancellationToken::new();
let t = token.clone();

std::thread::spawn(move || {
    for chunk in big_data.chunks(4096) {
        t.check()?;          // returns Err(Preempted) when cancelled
        process(chunk);
    }
    Ok(())
});

token.cancel_with("user requested stop");
```

### Level 2 — Watchdog timeout

```rust
use taskflow_rs::PreemptiveCancellationToken;
use std::time::Duration;

let token = PreemptiveCancellationToken::new();

// Automatically cancel after 500 ms — no manual polling needed.
token.cancel_after_with(Duration::from_millis(500), "budget exceeded");

// The task checks at its own pace:
let t = token.clone();
std::thread::spawn(move || {
    loop {
        t.check()?;
        do_work();
    }
    Ok(())
});
```

### Level 3 — RAII deadline guard

```rust
use taskflow_rs::PreemptiveCancellationToken;
use std::time::Duration;

let token = PreemptiveCancellationToken::new();

{
    // Guard cancels the token when this scope exits after the budget.
    let _guard = token.deadline_guard(Duration::from_millis(100));
    expensive_computation();
}   // ← token.cancel() fires here if elapsed > 100 ms

// Or use the scoped helper for a closure:
let result = taskflow_rs::with_deadline(Duration::from_secs(2), |tok| {
    for item in dataset {
        tok.check()?;
        process(item);
    }
    Ok(total)
});
```

### Token reuse

```rust
token.cancel_after(Duration::from_millis(50));
// ... run task ...
token.reset();   // clear the cancelled flag and reason
token.cancel_after(Duration::from_millis(50));
// ... run again ...
```

### Signal-based preemption (Linux)

```rust
// Call once at program startup:
unsafe { PreemptiveCancellationToken::install_signal_handler(); }

// In the task, check the per-thread SIGUSR2 flag:
PreemptiveCancellationToken::check_signal()?;

// From another thread, preempt a specific OS thread:
PreemptiveCancellationToken::signal_preempt_thread(pthread_id);
```

**Run the demo:**

```bash
cargo run --example preemptive_cancellation
```

---

## Dynamic Priority Adjustment

`SharedDynamicScheduler` allows the priority of any queued task to be changed in **O(log n)**
at any time, including from other threads.

### Basic usage

```rust
use taskflow_rs::{SharedDynamicScheduler, Priority};

let sched = SharedDynamicScheduler::new();

sched.push(1, Priority::Low);
sched.push(2, Priority::Normal);
let handle = sched.push(3, Priority::Low);

// Task 3 became urgent — escalate it before the executor picks it up.
handle.reprioritize(Priority::Critical);

// Pop order: 3 (Critical), 2 (Normal), 1 (Low)
while let Some(task_id) = sched.pop() {
    println!("executing {}", task_id);
}
```

### FIFO ordering within equal priority

Sequence numbers assigned at push time are preserved through reprioritization, so tasks at
the same priority level always execute in insertion order:

```rust
let h_old = sched.push(10, Priority::Normal);  // seq=0
let _h_new = sched.push(20, Priority::Normal); // seq=1

// Escalate both to High — seq=0 stays with task 10.
h_old.reprioritize(Priority::High);
sched.push(30, Priority::High); // seq=2

// Pop order: 10 (High, seq=0), 30 (High, seq=2), 20 (Normal)
```

### Cancelling a queued task

```rust
let handle = sched.push(99, Priority::High);

// Changed our mind — remove it before execution.
handle.cancel();
assert!(!handle.is_pending());
```

### Anti-starvation escalation policy

```rust
use taskflow_rs::EscalationPolicy;

let mut policy = EscalationPolicy::new(
    sched.clone(),
    /* tick_interval   */ 100,   // run escalation every 100 ticks
    /* low_age_ticks   */ 500,   // Low  → Normal after 500 ticks
    /* normal_age_ticks */ 1000, // Normal → High  after 1000 ticks
);

// Call inside the scheduler loop:
loop {
    policy.tick();
    if let Some(id) = sched.pop() {
        execute(id);
    }
}
```

**Run the demo:**

```bash
cargo run --example dynamic_priority
```

---

## Hardware Topology Integration

`TopologyProvider` exposes full hardware topology (NUMA nodes, CPU packages, L1/L2/L3
cache hierarchy) and can pin threads to specific CPU sets.

### Topology discovery

```rust
use taskflow_rs::{TopologyProvider, HwTopology};

let topo = TopologyProvider::detect();
// Uses hwloc2 when compiled with --features hwloc,
// falls back to /sys parsing otherwise.

println!("Backend:    {}", topo.backend_name());   // "hwloc2 2.2.0" or "sysfs-fallback"
println!("CPUs:       {}", topo.cpu_count());
println!("NUMA nodes: {}", topo.numa_nodes().len());

for node in topo.numa_nodes() {
    let mem = node.memory_bytes
        .map(|b| format!("{} MB", b / 1_048_576))
        .unwrap_or_else(|| "unknown".into());
    println!("  Node {}: {} CPUs, memory={}", node.id, node.cpus.len(), mem);
}

for pkg in topo.packages() {
    println!("  Socket {}: {} CPUs, NUMA={:?}", pkg.id, pkg.cpus.len(), pkg.numa_nodes);
}

for cache in topo.cache_info() {
    println!("  L{}: {} KB, line={} B, shared by {} CPUs",
        cache.level, cache.size_kb, cache.line_size, cache.shared_cpus.len());
}
```

### Worker CPU affinity

```rust
use taskflow_rs::{TopologyProvider, HwlocWorkerAffinity, AffinityStrategy};

let topo   = TopologyProvider::detect();
let num_workers = 8;
let affinity = HwlocWorkerAffinity::new(topo, AffinityStrategy::NUMADense, num_workers);

// Inside each worker thread at startup:
affinity.pin_current_thread(worker_id)?;

// Or just query the CPU set without binding:
let cpus = affinity.cpus_for_worker(worker_id);
println!("Worker {} → CPUs {:?}", worker_id, cpus);
```

Available strategies:

| Strategy | Behaviour |
|---|---|
| `None` | No binding; OS decides |
| `NUMARoundRobin` | Distribute workers evenly across NUMA nodes |
| `NUMADense` | Fill each NUMA node before moving to the next |
| `PhysicalCores` | Pin to physical cores; skip hyperthreading siblings |
| `L3CacheDomain` | Assign workers to L3 cache sharing groups |

### Enabling hwloc

```bash
# Ubuntu / Debian
sudo apt install libhwloc-dev

# Fedora / RHEL
sudo dnf install hwloc-devel

# macOS
brew install hwloc

# Build with full hwloc support
cargo build --features hwloc
```

Without `--features hwloc` the sysfs fallback is used automatically — no code changes
required. `topo.is_hwloc_backed()` returns `false` so callers can log the difference.

**Run the demo:**

```bash
cargo run --example hardware_topology             # sysfs fallback
cargo run --features hwloc --example hardware_topology  # full hwloc
```

---

## Static Task Priorities

For simpler use cases where priorities are fixed at enqueue time, use the built-in
`PriorityScheduler`:

```rust
use taskflow_rs::{PriorityScheduler, Scheduler, Priority};

let mut scheduler = PriorityScheduler::new();
scheduler.push(1, Priority::Low);
scheduler.push(2, Priority::High);
scheduler.push(3, Priority::Critical);

// Pop order: 3 (Critical), 2 (High), 1 (Low)
```

For live reprioritization use `SharedDynamicScheduler` instead — it implements the same
`Scheduler` trait and is a drop-in replacement.

---

## GPU Support

TaskFlow-RS provides a **backend-agnostic GPU API** supporting CUDA (NVIDIA), OpenCL
(NVIDIA/AMD/Intel), and ROCm/HIP (AMD).

### Building

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

# Stub backend (always available, no flags needed)
cargo build
```

### Usage

```rust
use taskflow_rs::{GpuDevice, GpuBuffer, BackendKind};

// Auto-select: CUDA → ROCm → OpenCL → Stub
let device = GpuDevice::new(0)?;

// Force a specific backend
let device = GpuDevice::with_backend(0, BackendKind::OpenCL)?;

// Allocate and transfer
let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&device, 1024)?;
buf.copy_from_host(&vec![1.0f32; 1024])?;

// Async transfer via stream
let stream = device.create_stream("compute")?;
unsafe { buf.copy_from_host_async(&src, &stream)?; }
stream.synchronize()?;
```

---

## Async Support

```toml
[dependencies]
taskflow-rs = { version = "0.2", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    taskflow.emplace_async(|| async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        println!("async task done");
    });

    executor.run_async(&taskflow).await;
}
```

---

## Tooling

```rust
use taskflow_rs::{Profiler, generate_dot_graph, PerformanceMetrics};

// Profiling
let profiler = Profiler::new(4);
profiler.enable();
executor.run(&taskflow).wait();
let profile = profiler.get_profile().unwrap();
println!("{}", profile.summary());

// Visualization
generate_dot_graph(&taskflow, "graph.dot");
// dot -Tsvg graph.dot -o graph.svg

// Real-time metrics
let metrics = PerformanceMetrics::new();
println!("Tasks/sec: {:.1}", metrics.tasks_per_second());
println!("Utilization: {:.1}%", metrics.worker_utilization() * 100.0);
```

---

## Building

```bash
# Core (no GPU, no hwloc)
cargo build

# With hwloc topology
cargo build --features hwloc

# With CUDA GPU
cargo build --features gpu

# Everything
cargo build --features hwloc,all-gpu,async

# Release
cargo build --release

# Tests
cargo test
cargo test --features hwloc
```

## Examples

```bash
# New scheduling features
cargo run --example preemptive_cancellation
cargo run --example dynamic_priority
cargo run --example hardware_topology
cargo run --features hwloc --example hardware_topology

# Advanced features (priorities, cooperative cancellation, NUMA)
cargo run --example advanced_features

# Async
cargo run --features async --example async_tasks
cargo run --features async --example async_parallel

# GPU (stub — no hardware required)
cargo run --example gpu_tasks
cargo run --example gpu_async_streams

# GPU with hardware
cargo run --features gpu --example gpu_tasks
cargo run --features gpu --example gpu_pipeline
```

---

## Architecture

### Work-stealing executor

```
Worker 0: [Task] [Task] [Task]  ← push/pop own queue (LIFO, cache-warm)
                ↓ steal (FIFO)
Worker 1: [Task] [Task]         ← idle workers steal from busy ones
```

### Scheduling layer

```
SharedDynamicScheduler
  ├── index:   BTreeMap<(RevPriority, SeqNum), TaskId>   O(log n) pop
  └── reverse: HashMap<TaskId, (RevPriority, SeqNum)>    O(1) lookup

PriorityHandle ──weak──► SharedDynamicScheduler
                          reprioritize() / cancel() without owning the queue

EscalationPolicy ──tick──► sched.reprioritize(id, bumped_priority)
                            anti-starvation for Low / Normal tasks
```

### Preemptive cancellation

```
PreemptiveCancellationToken
  ├── Arc<AtomicBool>  ← check() fast path: one Acquire load
  ├── Condvar          ← watchdog sleep / early wake on manual cancel
  └── cancel_after()   ── spawns watchdog thread
                           wait_timeout → drop(guard) → cancel_with_reason()
                                          ↑ guard dropped BEFORE notify
                                            (prevents self-deadlock)
```

### Hardware topology

```
TopologyProvider::detect()
  ├── --features hwloc → HwlocBackend  (hwloc2 = "2.2.0")
  └── default          → SysfsBackend  (/sys/devices/system/cpu/*/cache/)

HwlocWorkerAffinity
  ├── cpus_for_worker(id) → Vec<usize>
  └── pin_current_thread(id) → pthread_setaffinity_np (Linux)
                                hwloc set_cpubind THREAD (hwloc backend)
```

### GPU backend

```
GpuDevice  ──Arc<dyn ComputeBackend>──►  CudaBackend   (--features gpu)
                                          OpenCLBackend  (--features opencl)
                                          RocmBackend    (--features rocm)
                                          StubBackend    (always)
```

---

## Comparison with C++ TaskFlow

| Feature | C++ TaskFlow | TaskFlow-RS |
|---|:---:|:---:|
| Task Graphs | ✅ | ✅ |
| Work-Stealing | ✅ | ✅ |
| Subflows | ✅ | ✅ |
| Condition Tasks | ✅ | ✅ |
| Parallel Algorithms | ✅ | ✅ |
| Async Tasks | ✅ | ✅ |
| Pipeline | ✅ | ✅ |
| GPU — CUDA / OpenCL / ROCm | ✅ | ✅ |
| Async GPU Transfers | ✅ | ✅ |
| Multiple GPU Streams | ✅ | ✅ |
| Task Priorities | ✅ | ✅ |
| Cooperative Cancellation | ✅ | ✅ |
| Preemptive Cancellation | ✅ | ✅ |
| Dynamic Priority Adjustment | ✅ | ✅ |
| Hardware Topology (hwloc) | ✅ | ✅ |
| NUMA-Aware Scheduling | ✅ | ✅ |

---

## Documentation

| Document | Contents |
|---|---|
| [DESIGN.md](DESIGN.md) | Architecture, implementation status, design decisions |
| [ADVANCED_FEATURES.md](ADVANCED_FEATURES.md) | Priorities, cancellation, schedulers, NUMA |
| [GPU.md](GPU.md) | Full GPU API: backends, streams, async transfers |
| [GPU_SETUP.md](GPU_SETUP.md) | CUDA version config, ROCm install, troubleshooting |
| [ASYNC_TASKS.md](ASYNC_TASKS.md) | Async executor and task documentation |
| [PIPELINE.md](PIPELINE.md) | Concurrent pipeline documentation |
| [TOOLING.md](TOOLING.md) | Profiler, visualization, monitoring |

## License

MIT
