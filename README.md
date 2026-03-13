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
- ✅ **Real-time Dashboard** — HTTP server with SSE; live throughput chart and worker utilisation bars
- ✅ **Flamegraph Generation** — Interactive SVG flamegraphs from profiles or folded stacks
- ✅ **Automated Regression Detection** — Statistical baseline comparison with JSON persistence
- ✅ **Tooling** — Profiler, DOT/SVG/HTML visualization, performance monitoring, debug logging

---

## Quick Start

```toml
[dependencies]
taskflowrs = "0.1.0"
```

### Basic task graph

```rust
use taskflowrs::{Executor, Taskflow};

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

```rust
use taskflowrs::preemptive::PreemptiveCancellationToken;
use std::time::Duration;

let token = PreemptiveCancellationToken::new();

// Layer 1 — cooperative: task checks the flag manually
token.check()?;

// Layer 2 — watchdog: fires automatically after a deadline
token.cancel_after(Duration::from_millis(500));

// Layer 3 — RAII deadline guard (scope-bound)
let _guard = token.deadline_guard(Duration::from_secs(2));
```

---

## Real-time Dashboard

`DashboardServer` starts an HTTP server that streams live metrics to any browser via
Server-Sent Events. No external JavaScript libraries or CDN required — the page is
entirely self-contained.

```rust
use std::sync::Arc;
use taskflowrs::{
    monitoring::PerformanceMetrics,
    dashboard::{DashboardServer, DashboardConfig},
};

let metrics = Arc::new(PerformanceMetrics::new(4));
metrics.start();

let server = DashboardServer::new(
    Arc::clone(&metrics),
    4, // num_workers
    DashboardConfig { port: 9090, ..Default::default() },
);
let handle = server.start(); // non-blocking

println!("Dashboard → http://localhost:9090");

// … run your executor, record metrics …

handle.stop();
```

The dashboard provides:
- **Live throughput chart** (tasks/sec over a rolling 60 s window)
- **Worker utilisation bars** (colour-coded: green > 80%, yellow > 40%, red otherwise)
- **Stat cards** — tasks completed, throughput, avg task duration, steal rate
- **SSE history replay** — new clients immediately receive the full history buffer

---

## Flamegraph Generation

`FlamegraphGenerator` produces fully interactive SVG flamegraphs with zero external
dependencies. The output opens in any browser and supports click-to-zoom, search/highlight,
and hover tooltips.

### From an `ExecutionProfile`

```rust
use taskflowrs::flamegraph::{FlamegraphGenerator, FlamegraphConfig};

let gen = FlamegraphGenerator::new(FlamegraphConfig {
    title: "My Taskflow Run".to_string(),
    palette: "cool".to_string(), // "hot" | "cool" | "purple"
    ..Default::default()
});

gen.save_from_profile(&profile, "flamegraph.svg")?;
// open flamegraph.svg in any browser
```

### From folded stacks (perf / dtrace compatible)

```rust
// Standard "stack;frame N" format — compatible with perf-script | stackcollapse-perf
let folded = std::fs::read_to_string("perf.folded")?;
gen.save_from_folded(&folded, "flamegraph.svg")?;
```

SVG features:
- **Click** a frame to zoom into its subtree
- **Ctrl+click** (or press Reset) to restore the full view
- **Search box** dims non-matching frames in real time
- **Deterministic colours** — same frame name always gets the same colour

---

## Automated Regression Detection

`RegressionDetector` compares a finished `ExecutionProfile` against a stored `Baseline`
and produces a structured `RegressionReport` suitable for CI pipelines.

```rust
use taskflowrs::regression::{Baseline, RegressionDetector, RegressionThresholds};

// ── First run: record the baseline ──────────────────────────────────────────
let baseline = Baseline::from_profile(&profile, "v1.2.0");
baseline.save("perf_baseline.json")?;

// ── Subsequent runs: detect regressions ─────────────────────────────────────
let baseline = Baseline::load("perf_baseline.json")?;
let detector = RegressionDetector::new(baseline, RegressionThresholds::default());
let report   = detector.detect(&current_profile);

println!("{}", report.report());

// Fail CI on any regression
assert!(report.passed, "Performance regression detected!");

// Save JSON artefact for diff tracking
std::fs::write("regression_report.json", report.to_json())?;
```

### Threshold presets

| Preset | Total | Avg | P95 | P99 | Notes |
|---|---|---|---|---|---|
| `default()` | 10% | 10% | 15% | 20% | Balanced — recommended starting point |
| `strict()` | 5% | 5% | 8% | 10% | Critical paths; fail on small regressions |
| `lenient()` | 20% | 20% | 30% | 40% | Nightly builds; noise-tolerant |

Violations are classified as **WARNING** (> threshold) or **CRITICAL** (> 2× threshold).

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
use taskflowrs::{GpuDevice, GpuBuffer, BackendKind};

// Auto-select: CUDA → ROCm → OpenCL → Stub
let device = GpuDevice::new(0)?;

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
taskflowrs = { version = "0.1", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use taskflowrs::{AsyncExecutor, Taskflow};

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
use taskflowrs::{Profiler, generate_dot_graph, PerformanceMetrics};

// Profiling
let profiler = Profiler::new(4);
profiler.enable();
executor.run(&taskflow).wait();
let profile = profiler.get_profile().unwrap();
println!("{}", profile.summary());

// DOT / SVG / HTML visualization
generate_dot_graph(&tasks, &deps, "taskflow.dot");
// dot -Tsvg taskflow.dot -o taskflow.svg

// Flamegraph (new)
use taskflowrs::flamegraph::{FlamegraphGenerator, FlamegraphConfig};
FlamegraphGenerator::new(FlamegraphConfig::default())
    .save_from_profile(&profile, "flamegraph.svg")?;

// Real-time dashboard (new)
use taskflowrs::dashboard::{DashboardServer, DashboardConfig};
let handle = DashboardServer::new(metrics, 4, DashboardConfig::default()).start();
// → http://localhost:9090

// Regression detection (new)
use taskflowrs::regression::{Baseline, RegressionDetector, RegressionThresholds};
let baseline = Baseline::from_profile(&profile, "v1.0");
baseline.save("baseline.json")?;
let report = RegressionDetector::new(
    Baseline::load("baseline.json")?,
    RegressionThresholds::default(),
).detect(&new_profile);
assert!(report.passed);
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
# New tooling features
cargo run --example advanced_tooling_demo    # dashboard + flamegraph + regression

# Advanced scheduling
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

### Tooling layer

```
DashboardServer ──SSE──► browser (live charts, no CDN)
  └── DashboardConfig { port, push_interval_ms, history_len, title }

FlamegraphGenerator
  ├── from_profile(ExecutionProfile) → interactive SVG
  └── from_folded("a;b;c N")        → compatible with perf / dtrace

RegressionDetector
  ├── Baseline { total_us, avg_us, P50/P95/P99, efficiency } ← JSON file
  └── detect(profile) → RegressionReport { violations, summary, to_json }
```

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
| [TOOLING.md](TOOLING.md) | Profiler, visualization, monitoring, dashboard, flamegraphs, regression |

## License

MIT
