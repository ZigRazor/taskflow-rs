# TaskFlow-RS Design Document

## Architecture Overview

TaskFlow-RS is a task-parallel programming library built around directed acyclic graphs (DAGs) of
closures. Each node in the graph is a task; edges encode happens-before relationships. An executor
drives the graph to completion using a work-stealing thread pool. Three scheduling subsystems layer
on top of the core executor: a static priority queue, a dynamic reprioritization engine, and a
hardware-topology-aware affinity layer. A dedicated tooling layer provides real-time observability,
interactive flamegraphs, and automated regression detection.

```
┌──────────────────────────────────────────────────────────────┐
│                        User Code                              │
│   Taskflow (DAG)  ·  Pipeline  ·  Composition  ·  Async      │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│                   Scheduling Layer                             │
│  SharedDynamicScheduler  ·  EscalationPolicy                  │
│  PriorityScheduler  ·  FifoScheduler  ·  RoundRobinScheduler  │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│                    Executor (work-stealing)                    │
│  Per-worker lock-free deques  ·  Dependency counters          │
│  PreemptiveCancellationToken  ·  DeadlineGuard                │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│              Hardware Topology Layer                           │
│  TopologyProvider (hwloc2 / sysfs fallback)                   │
│  HwlocWorkerAffinity  ·  AffinityStrategy  ·  CacheInfo       │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│                     Tooling Layer                              │
│  DashboardServer (HTTP/SSE)  ·  FlamegraphGenerator           │
│  RegressionDetector  ·  Profiler  ·  PerformanceMetrics       │
│  DebugLogger  ·  DOT / SVG / HTML visualization               │
└──────────────────────────────────────────────────────────────┘
```

---

## Implementation Status

### Core

- [x] **Task Graph** ✅ COMPLETED
  - [x] DAG construction with `emplace` / `precede` / `succeed`
  - [x] Cycle detection
  - [x] Task naming and metadata
  - [x] Subflows (nested task graphs)
  - [x] Condition tasks (conditional branching)
  - [x] Loop constructs

- [x] **Executor** ✅ COMPLETED
  - [x] Work-stealing scheduler (crossbeam deque)
  - [x] Per-worker lock-free queues
  - [x] Configurable worker count (0 = auto)
  - [x] `run`, `run_n`, `run_until`, `run_many` variants
  - [x] Parallel `run_n` (N instances concurrently)
  - [x] `wait_for_all` barrier

- [x] **Parallel Algorithms** ✅ COMPLETED
  - [x] `parallel_for_each`
  - [x] `parallel_reduce`
  - [x] `parallel_transform`
  - [x] `parallel_sort`
  - [x] `parallel_inclusive_scan` / `parallel_exclusive_scan`

### Medium Priority

- [x] **Async Support** ✅ COMPLETED
  - [x] `AsyncExecutor` with Tokio integration
  - [x] `emplace_async` for `async fn` tasks
  - [x] `run_n_async`, `run_n_sequential_async`, `run_until_async`
  - [x] Shared state support with `Arc`/`Mutex`

- [x] **Pipeline** ✅ COMPLETED
  - [x] `ConcurrentPipeline` with token management
  - [x] Serial and parallel stages
  - [x] Backpressure / flow control
  - [x] `TypeSafePipeline` with compile-time type checking
  - [x] `SimplePipeline` for in-place mutations

- [x] **Composition** ✅ COMPLETED
  - [x] `Composition` / `CompositionBuilder` with entry/exit points
  - [x] `CloneableWork` for Arc-based task sharing
  - [x] `ParameterizedComposition` factory pattern
  - [x] `CompositionParams` with typed parameters (Int, Float, String, Bool)

- [x] **Run Variants** ✅ COMPLETED
  - [x] `run_n` — N sequential instances
  - [x] `run_until` — run until predicate
  - [x] `run_many` / `run_many_and_wait` — concurrent taskflows
  - [x] Thread-based parallel `run_n`

- [x] **Built-in Metrics** ✅ COMPLETED
  - [x] `Metrics` system with task execution tracking
  - [x] Worker utilization, tasks/sec, timing histogram
  - [x] Memory usage and success/failure rates

### Advanced Scheduling

- [x] **Task Priorities** ✅ COMPLETED
  - [x] `Priority` enum: `Low` / `Normal` / `High` / `Critical`
  - [x] Static priority queue (`PriorityScheduler`) with FIFO tie-breaking
  - [x] `FifoScheduler` and `RoundRobinScheduler` built-ins
  - [x] Pluggable `Scheduler` trait for custom strategies

- [x] **Cooperative Task Cancellation** ✅ COMPLETED
  - [x] `CancellationToken` — shared atomic flag, cheaply cloneable
  - [x] `is_cancelled()` / `cancel()` / `reset()` / `cancel_count()`

- [x] **Preemptive Cancellation** ✅ COMPLETED
  - [x] `PreemptiveCancellationToken` — extends cooperative model
  - [x] `cancel_after(Duration)` — watchdog thread fires at deadline
  - [x] `cancel_at(Instant)` — absolute-time deadline
  - [x] `cancel_with(reason)` — attaches a human-readable reason
  - [x] `check()` → `Result<(), Preempted>` — ergonomic `?` integration
  - [x] `check_and_yield()` — combines cancellation check with `yield_now()`
  - [x] `DeadlineGuard` — RAII scope-bound time budget
  - [x] `with_deadline(budget, closure)` — scoped deadline helper
  - [x] `reset()` — token reuse across multiple task runs
  - [x] Signal-based preemption via SIGUSR2 (Linux, opt-in)
  - [x] Watchdog self-deadlock-safe implementation (guard dropped before notify)

- [x] **Dynamic Priority Adjustment** ✅ COMPLETED
  - [x] `DynamicPriorityScheduler` — O(log n) reprioritization via dual-index
  - [x] `SharedDynamicScheduler` — thread-safe `Arc<Mutex<_>>` wrapper
  - [x] `PriorityHandle` — `Weak`-backed handle for deferred reprioritization
  - [x] `reprioritize(task_id, new_priority)` — live priority change
  - [x] FIFO ordering preserved within equal-priority tasks (sequence numbers)
  - [x] `remove(task_id)` — pre-execution cancellation via handle
  - [x] `snapshot()` — non-destructive priority-ordered view
  - [x] `EscalationPolicy` — tick-driven anti-starvation for Low / Normal tasks
  - [x] Implements `Scheduler` trait for drop-in executor integration

- [x] **Hardware Topology Integration** ✅ COMPLETED
  - [x] `TopologyProvider` — auto-selects hwloc2 or sysfs fallback at runtime
  - [x] `HwTopology` trait — backend-agnostic topology interface
  - [x] `HwlocBackend` (feature `hwloc`) — wraps `hwloc2 = "2.2.0"`
  - [x] `SysfsBackend` (always available) — reads `/sys/devices/system/cpu/*/cache/`
  - [x] NUMA node discovery with per-node memory capacity
  - [x] CPU package / socket enumeration
  - [x] L1 / L2 / L3 cache hierarchy: size, line size, associativity, shared CPUs
  - [x] `bind_thread(&[cpu])` — calls `pthread_setaffinity_np` (Linux) or hwloc bind
  - [x] `numa_node_for_cpu(cpu)` — O(1) CPU → NUMA node mapping
  - [x] `HwlocWorkerAffinity` — maps worker IDs to CPU sets
  - [x] Five `AffinityStrategy` variants: `None`, `NUMARoundRobin`, `NUMADense`,
        `PhysicalCores`, `L3CacheDomain`

- [x] **NUMA-Aware Scheduling** ✅ COMPLETED
  - [x] `NumaTopology::detect()` via sysfs
  - [x] `NumaPinning` strategies: None / RoundRobin / Dense / Sparse
  - [x] `get_worker_cpus(worker_id, num_workers, topology, strategy)`
  - [x] `TaskMetadata::with_numa_node(id)` hint

### Low Priority

- [x] **GPU Support** ✅ COMPLETED
  - [x] CUDA device integration via cudarc
  - [x] `GpuBuffer<T>` typed device buffer management
  - [x] Synchronous host↔device transfers
  - [x] Asynchronous transfers with `GpuStream` and `StreamPool`
  - [x] Multiple CUDA streams with `StreamSet` and `StreamGuard`
  - [x] OpenCL backend (`gpu_opencl.rs`) — NVIDIA / AMD / Intel
  - [x] ROCm/HIP backend (`gpu_rocm.rs`) — raw HIP FFI
  - [x] Stub backend — CI / no-GPU environments
  - [ ] Unified / pinned memory (planned)
  - [ ] Automatic kernel generation (planned)

- [x] **Tooling** ✅ COMPLETED
  - [x] `Profiler` with `ExecutionProfile` statistics
  - [x] DOT graph export, SVG timeline, HTML report
  - [x] Real-time `PerformanceMetrics` and worker utilization
  - [x] `DebugLogger` with structured log levels
  - [x] **Real-time dashboard** — HTTP server with SSE, self-contained HTML+JS UI (`src/dashboard.rs`)
  - [x] **Flamegraph generation** — interactive SVG from `ExecutionProfile` or folded stacks (`src/flamegraph.rs`)
  - [x] **Automated regression detection** — statistical baseline comparison with JSON persistence (`src/regression.rs`)

---

## Module Map

```
src/
├── lib.rs                  — public API surface
├── task.rs                 — Task, TaskHandle, TaskId
├── taskflow.rs             — Taskflow (DAG builder)
├── executor.rs             — Executor (work-stealing)
├── subflow.rs              — Subflow (nested graphs)
├── future.rs               — TaskflowFuture
├── condition.rs            — ConditionalHandle, BranchId, Loop
├── cycle_detection.rs      — CycleDetector, CycleDetectionResult
├── pipeline.rs             — ConcurrentPipeline, Token, StageType
├── typed_pipeline.rs       — TypeSafePipeline, PipelineBuilder, SimplePipeline
├── composition.rs          — Composition, CompositionBuilder, ParameterizedComposition
├── algorithms.rs           — parallel_for_each / reduce / transform / sort / scan
│
├── advanced.rs             — Priority, CancellationToken, TaskMetadata
├── scheduler.rs            — Scheduler trait, FifoScheduler, PriorityScheduler, RoundRobinScheduler
├── numa.rs                 — NumaTopology, NumaNode, NumaPinning
│
├── preemptive.rs           — PreemptiveCancellationToken, DeadlineGuard, Preempted
├── dynamic_priority.rs     — DynamicPriorityScheduler, SharedDynamicScheduler, PriorityHandle
├── escalation.rs           — EscalationPolicy
├── topology.rs             — TopologyProvider, HwTopology, HwlocBackend, SysfsBackend
├── affinity.rs             — HwlocWorkerAffinity, AffinityStrategy, CacheInfo
│
├── gpu.rs                  — GpuDevice, GpuBuffer, GpuTaskConfig (public API)
├── gpu_backend.rs          — ComputeBackend trait, BackendKind, DeviceBuffer
├── gpu_stream.rs           — GpuStream, StreamPool, StreamSet, StreamGuard
├── gpu_cuda_backend.rs     — CUDA backend (feature = "gpu")
├── gpu_opencl.rs           — OpenCL backend (feature = "opencl")
├── gpu_rocm.rs             — ROCm/HIP backend (feature = "rocm")
│
├── profiler.rs             — Profiler, ExecutionProfile, TaskStats
├── visualization.rs        — DOT / SVG / HTML export
├── monitoring.rs           — PerformanceMetrics, worker utilization
├── metrics.rs              — Metrics, MetricsSummary
├── debug.rs                — DebugLogger, LogLevel, LogEntry
├── dashboard.rs            — DashboardServer, DashboardHandle, DashboardConfig  ← NEW
├── flamegraph.rs           — FlamegraphGenerator, FlamegraphConfig              ← NEW
├── regression.rs           — RegressionDetector, Baseline, RegressionThresholds ← NEW
│
├── async_executor.rs       — AsyncExecutor (feature = "async")
└── loop_and_cycle.rs       — Loop detection and loop constructs
```

---

## Design Decisions

### Preemptive Cancellation: Three-Layer Model

True CPU preemption is unsafe in Rust's ownership model, so preemptive cancellation is
implemented as three escalating layers with the same token type:

```
Layer 1 — Cooperative (zero overhead on the fast path)
  task loop: token.check()?   →  Err(Preempted) if flag set

Layer 2 — Watchdog (background thread, ~1 ms granularity)
  token.cancel_after(Duration)  →  spawns watchdog thread
  token.cancel_at(Instant)      →  absolute deadline

Layer 3 — Signal (Linux only, microsecond latency, opt-in)
  PreemptiveCancellationToken::install_signal_handler()  (once at startup)
  token.signal_preempt_thread(pthread_id)                →  SIGUSR2
  token.check_signal()                                   →  thread-local flag
```

**Watchdog self-deadlock fix:** `Condvar::wait_timeout` re-acquires the `wakeup` mutex on
return. The watchdog must `drop(guard)` that re-acquired lock *before* calling
`cancel_with_reason`, which itself locks `wakeup` to notify waiters. Holding the lock across
the call causes the thread to block on a mutex it already owns.

### Pluggable GPU Backend (`ComputeBackend` trait)

The GPU layer is built around a single trait:

```rust
pub trait ComputeBackend: Send + Sync + fmt::Debug + 'static {
    fn kind(&self) -> BackendKind;
    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError>;
    fn htod_sync(&self, src: *const c_void, bytes: usize, dst: &DeviceBuffer) -> Result<(), GpuError>;
    fn dtoh_sync(&self, src: &DeviceBuffer, dst: *mut c_void, bytes: usize) -> Result<(), GpuError>;
    unsafe fn htod_async(..., stream: &DeviceStream) -> Result<(), GpuError>;
    unsafe fn dtoh_async(..., stream: &DeviceStream) -> Result<(), GpuError>;
    fn create_stream(&self) -> Result<DeviceStream, GpuError>;
    fn synchronize_device(&self) -> Result<(), GpuError>;
    fn memory_info(&self) -> Result<(usize, usize), GpuError>;
}
```

Backends are selected at **runtime** via `probe_backend()` (CUDA → ROCm → OpenCL → Stub).
The vtable cost is negligible compared to any real GPU operation and user code is
backend-agnostic — switching hardware requires changing one line.

### Real-time Dashboard (`src/dashboard.rs`)

The dashboard is a zero-dependency HTTP server built on `std::net::TcpListener`:

```
DashboardServer::start()
  ├── Listener thread   — accepts TCP, dispatches per-connection handlers
  ├── Collector thread  — samples PerformanceMetrics every N ms into a ring-buffer
  └── Per-connection handlers
        GET /         → self-contained HTML+JS+CSS (no CDN, no external assets)
        GET /events   → SSE stream (replays ring-buffer history, then live)
        GET /snapshot → one-shot JSON snapshot (polling fallback)
```

The embedded page uses the browser's native `EventSource` API and `<canvas>` for a live
throughput line chart and worker utilisation bar chart, all in ~150 lines of ES5 JavaScript.
`num_workers` is passed explicitly at construction time because `PerformanceMetrics::num_workers`
is a private field.

### Flamegraph Generation (`src/flamegraph.rs`)

Produces fully self-contained interactive SVG flamegraphs with zero runtime dependencies:

```
FlamegraphGenerator
  ├── from_profile(ExecutionProfile)  — tasks grouped per worker, depth from num_dependencies
  └── from_folded("a;b;c N" text)     — standard perf/dtrace folded-stacks format

FrameTree  — recursive Frame { name, total, self_samples, children }
  └── add_child() merges frames with identical names (aggregation)

build_svg() — produces interactive SVG with:
  • Click-to-zoom (narrows the x-axis to the selected subtree)
  • Ctrl+click resets to full view
  • Search/highlight by frame name (dims non-matching frames)
  • Deterministic colour palette: "hot" (red/orange), "cool" (blue/green), "purple"
  • Hover <title> tooltips: name + % total + self-time
```

Raw strings use `r##"..."##` throughout so that CSS/JS hex colour literals (`"#1a1d27"`)
never accidentally terminate the raw string delimiter.

### Automated Regression Detection (`src/regression.rs`)

Statistical regression detection with no external dependencies:

```
Baseline                               ← serialisable performance snapshot
  ├── from_profile(profile, label)     — captures total, avg, P50/P95/P99, efficiency
  ├── save(path) / load(path)          — hand-rolled JSON (no serde dep)
  └── to_json() / from_json()

RegressionThresholds                   ← per-metric % budgets
  ├── default()   10% total, 15% P95, 20% P99
  ├── strict()    5%  total, 8%  P95, 10% P99
  └── lenient()   20% total, 30% P95, 40% P99

RegressionDetector::detect(profile) → RegressionReport
  ├── comparisons[]  all metrics with baseline / current / Δ%
  ├── violations[]   WARNING (> threshold) or CRITICAL (> 2× threshold)
  ├── summary()      one-liner for CI stdout
  ├── report()       human-readable table
  └── to_json()      machine-readable artefact for diff storage
```

### `DynamicPriorityScheduler` dual-index

```
index:   BTreeMap<(RevPriority, SeqNum), TaskId>  O(log n) push / pop
reverse: HashMap<TaskId, (RevPriority, SeqNum)>   O(1) lookup for reprioritize

PriorityHandle ──weak──► SharedDynamicScheduler
                          reprioritize() / cancel() without owning the queue

EscalationPolicy ──tick──► sched.reprioritize(id, bumped_priority)
                            anti-starvation for Low / Normal tasks
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

### GPU backend probe

```
GpuDevice  ──Arc<dyn ComputeBackend>──►  CudaBackend   (--features gpu)
                                          OpenCLBackend  (--features opencl)
                                          RocmBackend    (--features rocm)
                                          StubBackend    (always)
```

---

## Concurrency and Safety Guarantees

| Component | Synchronization | Notes |
|---|---|---|
| `PreemptiveCancellationToken` | `Arc<AtomicBool>` + `Condvar` | Clone is O(1), check is one `Acquire` load |
| `SharedDynamicScheduler` | `Arc<Mutex<_>>` | Lock held only during push/pop/reprioritize |
| `PriorityHandle` | `Weak<Mutex<_>>` | No strong reference; scheduler can be dropped freely |
| `EscalationPolicy` | single-threaded tick | Intended for the scheduler loop thread only |
| `TopologyProvider` | immutable after `detect()` | `bind_thread` is per-thread, no shared state |
| Work-stealing deques | `crossbeam::deque` | Lock-free; LIFO own-queue, FIFO steal |
| Task dependency counters | `AtomicUsize` | `Release` on decrement, `Acquire` on zero-check |
| `DashboardServer` collector | `Arc<Mutex<VecDeque>>` | Lock held only during ring-buffer push/pop |
| `RegressionDetector` | immutable after construction | `detect()` takes `&self`, fully re-entrant |

---

## Performance Characteristics

| Operation | Complexity | Notes |
|---|---|---|
| `DynamicPriorityScheduler::push` | O(log n) | BTreeMap insert + HashMap insert |
| `DynamicPriorityScheduler::pop` | O(log n) | BTreeMap first entry removal |
| `DynamicPriorityScheduler::reprioritize` | O(log n) | Two BTree ops, two HashMap ops |
| `DynamicPriorityScheduler::remove` | O(log n) | Same as reprioritize |
| `PreemptiveCancellationToken::check` | O(1) | Single `Acquire` atomic load |
| `PreemptiveCancellationToken::cancel_after` | O(1) | Spawns one background thread |
| `TopologyProvider::detect` | one-time | Amortized across all workers |
| `HwlocWorkerAffinity::cpus_for_worker` | O(nodes) | Proportional to NUMA node count |
| `bind_thread` (Linux) | O(cpus) | One `pthread_setaffinity_np` syscall |
| `FlamegraphGenerator::from_profile` | O(T log T) | Sort tasks by start time per worker |
| `RegressionDetector::detect` | O(T log T) | Dominated by percentile sort |
| Dashboard SSE push | O(1) | Append to ring-buffer, no global lock on hot path |

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
| Real-time Dashboard | ➖ | ✅ |
| Interactive Flamegraphs | ➖ | ✅ |
| Automated Regression Detection | ➖ | ✅ |

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

---

## Future Enhancements

- Priority inheritance for dependent tasks (Critical task promotes its blockers)
- Real-time scheduling support (SCHED_FIFO / SCHED_RR integration)
- Automatic NUMA memory allocation alignment with worker topology
- Signal-based preemption on macOS (thread port messaging)
- Per-cache-domain work queues for L3-local task stealing
- Unified / pinned GPU memory
- Automatic GPU kernel generation
- Automatic profiler integration into executor (zero-instrumentation profiling)
- JSON export format for `ExecutionProfile`
- Integration with external monitoring systems (Prometheus, OpenTelemetry)

## License

MIT
